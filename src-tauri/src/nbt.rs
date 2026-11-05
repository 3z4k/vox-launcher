//! A small NBT reader/writer (big-endian, as used by servers.dat and level.dat).

use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// element type id + items (the id is kept so empty lists survive a round trip)
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Tag::String(s) = self { Some(s) } else { None }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Tag::Byte(v) => Some(*v as i64),
            Tag::Short(v) => Some(*v as i64),
            Tag::Int(v) => Some(*v as i64),
            Tag::Long(v) => Some(*v),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Tag) {
        if let Tag::Compound(items) = self {
            match items.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => *v = value,
                None => items.push((key.to_string(), value)),
            }
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.p + n > self.b.len() {
            bail!("Unexpected end of NBT data");
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn string(&mut self) -> Result<String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into()?) as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() {
            bail!("Invalid NBT length");
        }
        Ok(n as usize)
    }
    fn payload(&mut self, id: u8, depth: u32) -> Result<Tag> {
        if depth > 64 {
            bail!("NBT nested too deeply");
        }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.iter().map(|b| *b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let t = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.payload(t, depth + 1)?);
                }
                Tag::List(t, v)
            }
            10 => {
                let mut items = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let name = self.string()?;
                    items.push((name, self.payload(t, depth + 1)?));
                }
                Tag::Compound(items)
            }
            11 => {
                let n = self.len()?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_>>()?)
            }
            12 => {
                let n = self.len()?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_>>()?)
            }
            other => bail!("Unknown NBT tag {other}"),
        })
    }
}

/// Parses an uncompressed NBT document; returns the root tag (usually a compound).
pub fn parse(bytes: &[u8]) -> Result<Tag> {
    let mut r = Reader { b: bytes, p: 0 };
    let id = r.u8()?;
    if id != 10 {
        bail!("NBT root is not a compound");
    }
    let _root_name = r.string()?;
    r.payload(id, 0)
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    out.extend((s.len() as u16).to_be_bytes());
    out.extend(s.as_bytes());
}

fn put_payload(out: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend(v.to_be_bytes()),
        Tag::Int(v) => out.extend(v.to_be_bytes()),
        Tag::Long(v) => out.extend(v.to_be_bytes()),
        Tag::Float(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend(v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            out.extend(v.iter().map(|b| *b as u8));
        }
        Tag::String(s) => put_string(out, s),
        Tag::List(id, items) => {
            out.push(if items.is_empty() { *id } else { items[0].id() });
            out.extend((items.len() as i32).to_be_bytes());
            for i in items {
                put_payload(out, i);
            }
        }
        Tag::Compound(items) => {
            for (k, v) in items {
                out.push(v.id());
                put_string(out, k);
                put_payload(out, v);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            for i in v {
                out.extend(i.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend((v.len() as i32).to_be_bytes());
            for i in v {
                out.extend(i.to_be_bytes());
            }
        }
    }
}

/// Serialises a root compound with an empty root name.
pub fn write(root: &Tag) -> Vec<u8> {
    let mut out = vec![10];
    put_string(&mut out, "");
    put_payload(&mut out, root);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tag {
        Tag::Compound(vec![
            ("name".into(), Tag::String("Hello".into())),
            ("n".into(), Tag::Int(-5)),
            ("big".into(), Tag::Long(1_700_000_000_000)),
            ("flag".into(), Tag::Byte(1)),
            ("pi".into(), Tag::Double(3.25)),
            ("f".into(), Tag::Float(1.5)),
            ("ints".into(), Tag::IntArray(vec![1, 2, 3])),
            ("bytes".into(), Tag::ByteArray(vec![-1, 0, 1])),
            ("empty".into(), Tag::List(10, vec![])),
            ("list".into(), Tag::List(10, vec![Tag::Compound(vec![("x".into(), Tag::Short(7))])])),
        ])
    }

    #[test]
    fn round_trips_every_tag_type() {
        let bytes = write(&sample());
        assert_eq!(parse(&bytes).unwrap(), sample());
    }

    #[test]
    fn helpers_and_errors() {
        let t = sample();
        assert_eq!(t.get("name").and_then(Tag::as_str), Some("Hello"));
        assert_eq!(t.get("big").and_then(Tag::as_i64), Some(1_700_000_000_000));
        let mut t = t;
        t.set("n", Tag::Int(9));
        t.set("added", Tag::Byte(0));
        assert_eq!(t.get("n"), Some(&Tag::Int(9)));
        assert!(t.get("added").is_some());
        assert!(parse(&[]).is_err());
        assert!(parse(&[10, 0, 0, 3, 0, 1]).is_err());
        let bytes = write(&sample());
        assert!(parse(&bytes[..bytes.len() - 3]).is_err(), "truncated data must error, not panic");
    }
}
