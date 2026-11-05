//! Discord Rich Presence. Best effort: if Discord is not running, or no application ID is configured,
//! everything here quietly does nothing and retries on the next update. Runs on its own thread because
//! the IPC client is blocking.

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Game {
    pub details: String,
    pub state: String,
    pub started_at_ms: i64,
    /// Server the player is connected to, when known.
    pub server: Option<String>,
}

enum Msg {
    Update { enabled: bool, app_id: String, game: Option<Game> },
    Shutdown,
}

pub struct Discord {
    tx: Sender<Msg>,
    games: Mutex<Vec<(String, Game)>>, // launch order
    started_at_ms: i64,
}

impl Discord {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<Msg>();
        std::thread::spawn(move || {
            let mut client: Option<(String, DiscordIpcClient)> = None;
            let drop_client = |c: &mut Option<(String, DiscordIpcClient)>| {
                if let Some((_, mut cl)) = c.take() {
                    let _ = cl.close();
                }
            };
            while let Ok(msg) = rx.recv() {
                // Only the newest queued update matters.
                let mut msg = msg;
                while let Ok(next) = rx.try_recv() {
                    msg = next;
                }
                match msg {
                    Msg::Shutdown => {
                        drop_client(&mut client);
                        return;
                    }
                    Msg::Update { enabled, app_id, game } => {
                        if !enabled || app_id.is_empty() {
                            drop_client(&mut client);
                            continue;
                        }
                        if client.as_ref().is_some_and(|(id, _)| id != &app_id) {
                            drop_client(&mut client);
                        }
                        if client.is_none() {
                            let mut c = DiscordIpcClient::new(&app_id);
                            if c.connect().is_err() {
                                continue; // Discord is not running
                            }
                            client = Some((app_id.clone(), c));
                        }
                        let Some((_, c)) = client.as_mut() else { continue };
                        let game = game.unwrap_or(Game { details: "In the launcher".into(), state: "Managing instances".into(), started_at_ms: 0, server: None });
                        let mut act = activity::Activity::new()
                            .details(&game.details)
                            .state(&game.state)
                            .assets(activity::Assets::new().large_image("vox").large_text("Vox Launcher"));
                        if game.started_at_ms > 0 {
                            act = act.timestamps(activity::Timestamps::new().start(game.started_at_ms / 1000));
                        }
                        if c.set_activity(act).is_err() {
                            drop_client(&mut client); // reconnect on the next update
                        }
                    }
                }
            }
        });
        Discord { tx, games: Mutex::new(Vec::new()), started_at_ms: crate::util::now_ms() as i64 }
    }

    /// Pushes the right activity to Discord, or clears it. Never fails.
    pub fn refresh(&self, enabled: bool, app_id: &str, show_time: bool, show_server: bool) {
        let latest = self.games.lock().unwrap().last().map(|(_, g)| {
            let mut g = g.clone();
            if show_server {
                if let Some(s) = g.server.take() {
                    g.state = format!("Server: {s}");
                }
            }
            g
        });
        let idle_start = if show_time { self.started_at_ms } else { 0 };
        let game = latest.or(Some(Game { details: "In the launcher".into(), state: "Managing instances".into(), started_at_ms: idle_start, server: None }));
        let _ = self.tx.send(Msg::Update { enabled, app_id: app_id.to_string(), game });
    }

    pub fn game_started(&self, id: &str, game: Game) {
        let mut g = self.games.lock().unwrap();
        g.retain(|(i, _)| i != id);
        g.push((id.to_string(), game));
    }

    /// Reads one game log line; returns true when the known server changed.
    pub fn observe_log(&self, id: &str, line: &str) -> bool {
        let next = server_from_log(line);
        let Some(next) = next else { return false };
        let mut g = self.games.lock().unwrap();
        match g.iter_mut().find(|(i, _)| i == id) {
            Some((_, game)) if game.server != next => {
                game.server = next;
                true
            }
            _ => false,
        }
    }

    pub fn game_stopped(&self, id: &str) {
        self.games.lock().unwrap().retain(|(i, _)| i != id);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

/// `Some(Some(host))` when the line says the client is connecting to a server, `Some(None)` when it starts a
/// singleplayer world (so a stale server is cleared), `None` for every other line.
pub fn server_from_log(line: &str) -> Option<Option<String>> {
    if let Some(i) = line.find("]: Connecting to ") {
        let rest = &line[i + "]: Connecting to ".len()..];
        let host = rest.split(',').next()?.trim();
        if host.is_empty() || host.len() > 80 || host.contains(' ') {
            return None;
        }
        return Some(Some(host.to_string()));
    }
    if line.contains("Starting integrated minecraft server") {
        return Some(None);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_lines_are_recognised() {
        assert_eq!(server_from_log("[12:00:01] [Render thread/INFO]: Connecting to mc.hypixel.net, 25565"), Some(Some("mc.hypixel.net".into())));
        assert_eq!(server_from_log("[12:00:01] [Render thread/INFO]: Starting integrated minecraft server version 1.21.4"), Some(None));
        assert_eq!(server_from_log("[12:00:01] [main/INFO]: Setting user: Steve"), None);
        assert_eq!(server_from_log("[x]: Connecting to , 25565"), None);
    }

    #[test]
    fn observe_updates_only_known_games() {
        let d = Discord::spawn();
        assert!(!d.observe_log("a", "[t]: Connecting to x.y, 1"));
        d.game_started("a", Game { details: "d".into(), state: "s".into(), started_at_ms: 0, server: None });
        assert!(d.observe_log("a", "[t]: Connecting to x.y, 1"));
        assert!(!d.observe_log("a", "[t]: Connecting to x.y, 1"));
        assert!(d.observe_log("a", "[t]: Starting integrated minecraft server version 1"));
        d.shutdown();
    }
}
