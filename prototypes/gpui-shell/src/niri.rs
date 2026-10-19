//! Read-only adapter for the Niri command-line JSON protocol.
//!
//! Niri's event stream is the long-term source for low-latency updates. This
//! prototype polls its documented snapshots because it does not yet own a
//! persistent socket/event-stream client.

use std::collections::{BTreeMap, HashSet};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: u64,
    pub number: u8,
    pub name: Option<String>,
    pub occupied: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub active_window: Option<String>,
    pub clock: Option<String>,
    pub error: Option<String>,
}

/// Live Niri window metadata needed by app surfaces such as Iris Orbit and Dock.
#[derive(Clone, Debug)]
pub struct Window {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub workspace_id: Option<u64>,
    pub urgent: bool,
    pub focused: bool,
    /// Whether Niri placed the window in its floating layout.
    pub floating: Option<bool>,
    /// Niri's two-dimensional tile position, when reported by the window JSON.
    pub pos_in_scrolling_layout: Option<[u64; 2]>,
    /// Niri's logical tile width and height, including its layout allocation.
    pub tile_size: Option<[f32; 2]>,
    /// Niri's output-local tile origin in the current workspace view.
    /// This is most useful for floating windows; tiled windows may omit it.
    pub tile_pos_in_workspace_view: Option<[f32; 2]>,
}

pub fn snapshot(output_name: Option<&str>) -> Snapshot {
    let mut result = Snapshot::default();
    let windows_json = command_json(&["msg", "-j", "windows"]);
    let occupied_ids = match &windows_json {
        Ok(Json::Array(entries)) => entries
            .iter()
            .filter_map(|entry| entry.get("workspace_id")?.as_u64())
            .collect::<HashSet<_>>(),
        _ => HashSet::new(),
    };
    match windows_json {
        Ok(Json::Array(entries)) => {
            result.windows = entries
                .iter()
                .filter_map(|entry| {
                    Some(Window {
                        id: entry.get("id")?.as_u64()?,
                        app_id: entry
                            .get("app_id")
                            .and_then(Json::as_str)
                            .map(str::to_owned),
                        title: entry.get("title").and_then(Json::as_str).map(str::to_owned),
                        workspace_id: entry.get("workspace_id").and_then(Json::as_u64),
                        urgent: entry
                            .get("is_urgent")
                            .and_then(Json::as_bool)
                            .unwrap_or(false),
                        focused: entry
                            .get("is_focused")
                            .and_then(Json::as_bool)
                            .unwrap_or(false),
                        floating: entry.get("is_floating").and_then(Json::as_bool),
                        pos_in_scrolling_layout: entry
                            .get("layout")
                            .and_then(|layout| layout.get("pos_in_scrolling_layout"))
                            .and_then(Json::as_array)
                            .and_then(|position| {
                                if position.len() != 2 {
                                    return None;
                                }
                                Some([position.first()?.as_u64()?, position.get(1)?.as_u64()?])
                            }),
                        tile_size: entry
                            .get("layout")
                            .and_then(|layout| layout.get("tile_size"))
                            .and_then(Json::as_array)
                            .and_then(|size| {
                                if size.len() != 2 {
                                    return None;
                                }
                                let width = size.first()?.as_f64()? as f32;
                                let height = size.get(1)?.as_f64()? as f32;
                                (width.is_finite()
                                    && height.is_finite()
                                    && width > 0.0
                                    && height > 0.0)
                                    .then_some([width, height])
                            }),
                        tile_pos_in_workspace_view: entry
                            .get("layout")
                            .and_then(|layout| layout.get("tile_pos_in_workspace_view"))
                            .and_then(Json::as_array)
                            .and_then(|position| {
                                if position.len() != 2 {
                                    return None;
                                }
                                let x = position.first()?.as_f64()? as f32;
                                let y = position.get(1)?.as_f64()? as f32;
                                (x.is_finite() && y.is_finite()).then_some([x, y])
                            }),
                    })
                })
                .collect();
        }
        Ok(_) => result.error = Some("Niri returned an unexpected windows response".into()),
        Err(error) => result.error = Some(error),
    }
    match command_json(&["msg", "-j", "workspaces"]) {
        Ok(Json::Array(entries)) => {
            result.workspaces = entries
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("idx")?.as_u64()?;
                    let number = u8::try_from(id).ok()?;
                    if output_name.is_some_and(|name| {
                        entry.get("output").and_then(Json::as_str) != Some(name)
                    }) {
                        return None;
                    }
                    Some(Workspace {
                        id: entry.get("id")?.as_u64()?,
                        number,
                        name: entry.get("name").and_then(Json::as_str).map(str::to_owned),
                        occupied: entry
                            .get("id")
                            .and_then(Json::as_u64)
                            .is_some_and(|id| occupied_ids.contains(&id)),
                        active: entry
                            .get("is_focused")
                            .and_then(Json::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect();
            result.workspaces.sort_by_key(|workspace| workspace.number);
        }
        Ok(_) => result.error = Some("Niri returned an unexpected workspace response".into()),
        Err(error) => result.error = Some(error),
    }

    match command_json(&["msg", "-j", "focused-window"]) {
        Ok(window) => {
            result.active_window = window
                .get("title")
                .and_then(Json::as_str)
                .filter(|title| !title.trim().is_empty())
                .map(str::to_owned);
        }
        Err(error) if result.error.is_none() => result.error = Some(error),
        Err(_) => {}
    }

    result.clock = Command::new("date")
        .arg("+%H:%M · %a, %b %-d")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    result
}

/// Ask Niri to focus a known window id. Returns false when Niri is unavailable
/// or rejects the request; callers should keep the existing focus unchanged.
pub fn focus_window(id: u64) -> bool {
    Command::new("niri")
        .args(["msg", "action", "focus-window", "--id", &id.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

/// Switch to a real Niri workspace by its reported numeric index.
pub fn focus_workspace(number: u8) -> bool {
    Command::new("niri")
        .args(["msg", "action", "focus-workspace"])
        .arg(number.to_string())
        .status()
        .is_ok_and(|status| status.success())
}

/// Close a specific Niri window by its reported id.
pub fn close_window(id: u64) -> bool {
    Command::new("niri")
        .args(["msg", "action", "close-window", "--id", &id.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

fn command_json(args: &[&str]) -> Result<Json, String> {
    let output = Command::new("niri")
        .args(args)
        .output()
        .map_err(|error| format!("Niri unavailable: {error}"))?;
    if !output.status.success() {
        return Err("Niri is not responding".into());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| "Niri returned invalid UTF-8")?;
    JsonParser::new(text)
        .parse()
        .map_err(|_| "Niri returned invalid JSON".into())
}

#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(fields) => fields.get(key),
            _ => None,
        }
    }
    fn as_u64(&self) -> Option<u64> {
        if let Self::Number(value) = self {
            if value.is_finite()
                && *value >= 0.0
                && value.fract() == 0.0
                && *value <= u64::MAX as f64
            {
                Some(*value as u64)
            } else {
                None
            }
        } else {
            None
        }
    }
    fn as_f64(&self) -> Option<f64> {
        if let Self::Number(value) = self {
            value.is_finite().then_some(*value)
        } else {
            None
        }
    }
    fn as_array(&self) -> Option<&[Json]> {
        if let Self::Array(values) = self {
            Some(values)
        } else {
            None
        }
    }
    fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    fn as_str(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            cursor: 0,
        }
    }
    fn parse(mut self) -> Result<Json, ()> {
        let value = self.value()?;
        self.space();
        if self.cursor == self.bytes.len() {
            Ok(value)
        } else {
            Err(())
        }
    }
    fn value(&mut self) -> Result<Json, ()> {
        self.space();
        match self.peek().ok_or(())? {
            b'n' => {
                self.word(b"null")?;
                Ok(Json::Null)
            }
            b't' => {
                self.word(b"true")?;
                Ok(Json::Bool(true))
            }
            b'f' => {
                self.word(b"false")?;
                Ok(Json::Bool(false))
            }
            b'"' => self.string().map(Json::String),
            b'[' => self.array(),
            b'{' => self.object(),
            b'0'..=b'9' => self.number(),
            _ => Err(()),
        }
    }
    fn array(&mut self) -> Result<Json, ()> {
        self.cursor += 1;
        let mut values = Vec::new();
        loop {
            self.space();
            if self.take(b']') {
                return Ok(Json::Array(values));
            }
            if !values.is_empty() {
                self.expect(b',')?;
            }
            values.push(self.value()?);
        }
    }
    fn object(&mut self) -> Result<Json, ()> {
        self.cursor += 1;
        let mut fields = BTreeMap::new();
        loop {
            self.space();
            if self.take(b'}') {
                return Ok(Json::Object(fields));
            }
            if !fields.is_empty() {
                self.expect(b',')?;
            }
            self.space();
            let key = self.string()?;
            self.expect(b':')?;
            fields.insert(key, self.value()?);
        }
    }
    fn number(&mut self) -> Result<Json, ()> {
        let start = self.cursor;
        self.take(b'-');
        if !self.take(b'0') {
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(());
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.cursor += 1;
            }
        }
        if self.take(b'.') {
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(());
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.cursor += 1;
            }
        }
        if self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            self.cursor += 1;
            if self.peek().is_some_and(|b| b == b'+' || b == b'-') {
                self.cursor += 1;
            }
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(());
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.cursor += 1;
            }
        }
        let value = std::str::from_utf8(&self.bytes[start..self.cursor])
            .map_err(|_| ())?
            .parse()
            .map_err(|_| ())?;
        Ok(Json::Number(value))
    }
    fn string(&mut self) -> Result<String, ()> {
        self.expect(b'"')?;
        let mut value = String::new();
        while let Some(byte) = self.peek() {
            self.cursor += 1;
            match byte {
                b'"' => return Ok(value),
                b'\\' => {
                    let escaped = self.peek().ok_or(())?;
                    self.cursor += 1;
                    match escaped {
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        b'/' => value.push('/'),
                        b'b' => value.push('\u{8}'),
                        b'f' => value.push('\u{c}'),
                        b'n' => value.push('\n'),
                        b'r' => value.push('\r'),
                        b't' => value.push('\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xd800..=0xdbff).contains(&code) {
                                if !self.take(b'\\') || !self.take(b'u') {
                                    return Err(());
                                }
                                let low = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&low) {
                                    return Err(());
                                }
                                code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                            }
                            let ch = char::from_u32(code).ok_or(())?;
                            value.push(ch);
                        }
                        _ => return Err(()),
                    }
                }
                0..=31 => return Err(()),
                32..=127 => value.push(char::from(byte)),
                _ => {
                    let start = self.cursor - 1;
                    let tail = std::str::from_utf8(&self.bytes[start..]).map_err(|_| ())?;
                    let ch = tail.chars().next().ok_or(())?;
                    value.push(ch);
                    self.cursor = start + ch.len_utf8();
                }
            }
        }
        Err(())
    }
    fn hex4(&mut self) -> Result<u32, ()> {
        let end = self.cursor + 4;
        let text =
            std::str::from_utf8(self.bytes.get(self.cursor..end).ok_or(())?).map_err(|_| ())?;
        let value = u32::from_str_radix(text, 16).map_err(|_| ())?;
        self.cursor = end;
        Ok(value)
    }
    fn word(&mut self, word: &[u8]) -> Result<(), ()> {
        if self.bytes.get(self.cursor..self.cursor + word.len()) == Some(word) {
            self.cursor += word.len();
            Ok(())
        } else {
            Err(())
        }
    }
    fn expect(&mut self, byte: u8) -> Result<(), ()> {
        if self.take(byte) { Ok(()) } else { Err(()) }
    }
    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }
    fn space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.cursor += 1;
        }
    }
}
