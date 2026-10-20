//! Blocking `cliphist`/`wl-copy` adapter for callers running on a background
//! executor. No command is passed through a shell.
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

const MAX_ENTRIES: usize = 400;
const MAX_PREVIEW_CHARS: usize = 240;
const PREVIEW_LIMIT: usize = 100;
const MAX_PIN_CHARS: usize = 8_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: String,
    pub preview: String,
    /// Original `cliphist list` row, retained for `cliphist delete` stdin.
    pub source_row: String,
    pub is_image: bool,
    pub pinnable: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredPin {
    pub id: String,
    pub text: String,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub entries: Vec<HistoryEntry>,
    pub pins: Vec<ConfiguredPin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderError {
    /// The executable is absent or cannot be launched due to a missing runtime.
    Unavailable { executable: PathBuf, detail: String },
    /// The process started but failed, or IO with it failed.
    Command {
        executable: PathBuf,
        operation: &'static str,
        status: Option<i32>,
        detail: String,
    },
    /// `cliphist list` emitted a non-empty row outside the expected format.
    Parse { line: usize, detail: String },
}

#[derive(Clone, Debug)]
pub struct CliphistProvider {
    cliphist: PathBuf,
    wl_copy: PathBuf,
}

impl Default for CliphistProvider {
    fn default() -> Self {
        Self::new("cliphist", "wl-copy")
    }
}

impl CliphistProvider {
    pub fn new(cliphist: impl Into<PathBuf>, wl_copy: impl Into<PathBuf>) -> Self {
        Self {
            cliphist: cliphist.into(),
            wl_copy: wl_copy.into(),
        }
    }

    /// Fetch at most 400 entries and map externally-owned config pins onto
    /// matching previews. All process and pipe IO here is blocking.
    pub fn list(&self, configured_pins: &[String]) -> Result<Snapshot, ProviderError> {
        let output = self.run(&self.cliphist, "list", &[], None)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            if entries.len() >= MAX_ENTRIES {
                break;
            }
            if raw.is_empty() {
                continue;
            }
            let Some((id, preview)) = raw.split_once('\t') else {
                return Err(ProviderError::Parse {
                    line: index + 1,
                    detail: "expected id<TAB>preview".into(),
                });
            };
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ProviderError::Parse {
                    line: index + 1,
                    detail: "entry id must contain decimal digits".into(),
                });
            }
            let is_image = image_placeholder(preview);
            let preview = sanitize_preview(preview);
            let pinned = configured_pins
                .iter()
                .any(|pin| preview_matches_pin(&preview, pin));
            entries.push(HistoryEntry {
                id: id.to_owned(),
                preview,
                source_row: raw.to_owned(),
                is_image,
                pinnable: !is_image,
                pinned,
            });
        }
        let pins = configured_pins
            .iter()
            .take(200)
            .enumerate()
            .filter_map(|(index, text)| {
                let text: String = text.chars().take(MAX_PIN_CHARS).collect();
                (!text.is_empty()).then(|| ConfiguredPin {
                    id: format!("configured-pin-{index}"),
                    preview: sanitize_preview(
                        text.lines()
                            .find(|line| !line.trim().is_empty())
                            .unwrap_or(&text),
                    ),
                    text,
                })
            })
            .collect();
        Ok(Snapshot { entries, pins })
    }

    /// Decode an entry by its numeric history id, then copy the exact bytes to
    /// `wl-copy` stdin. Binary/image entries are supported.
    pub fn copy(&self, id: &str) -> Result<(), ProviderError> {
        validate_id(id)?;
        let mut decoded = self.spawn_stdout(&self.cliphist, "decode", &[id])?;
        let mut copier = Command::new(&self.wl_copy)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| launch_error(&self.wl_copy, e))?;
        let mut stdin = copier.stdin.take().expect("stdin was configured");
        let mut decoded_stdout = decoded.stdout.take().expect("stdout was configured");
        let copy_result = io::copy(&mut decoded_stdout, &mut stdin);
        drop(stdin);
        let decode_status = decoded
            .wait()
            .map_err(|e| command_error(&self.cliphist, "decode", None, e.to_string()))?;
        if !decode_status.success() {
            let status = decode_status.code();
            let stderr = decoded
                .stderr
                .as_mut()
                .map(read_child_stderr)
                .unwrap_or_default();
            let _ = copier.kill();
            let _ = copier.wait();
            return Err(command_error(&self.cliphist, "decode", status, stderr));
        }
        if let Err(error) = copy_result {
            let _ = copier.kill();
            let _ = copier.wait();
            return Err(command_error(
                &self.wl_copy,
                "copy stdin",
                None,
                error.to_string(),
            ));
        }
        let status = copier
            .wait()
            .map_err(|e| command_error(&self.wl_copy, "copy", None, e.to_string()))?;
        if !status.success() {
            return Err(command_error(
                &self.wl_copy,
                "copy",
                status.code(),
                copier
                    .stderr
                    .as_mut()
                    .map(read_child_stderr)
                    .unwrap_or_default(),
            ));
        }
        Ok(())
    }

    /// Decode a text entry for the Material clipboard's browser-markup filter.
    /// The caller decides whether the payload is text; image rows should use
    /// `copy` so their bytes remain untouched.
    pub fn decode_for_copy(&self, id: &str) -> Result<Vec<u8>, ProviderError> {
        validate_id(id)?;
        self.run(&self.cliphist, "decode", &[id], None)
            .map(|output| output.stdout)
    }

    /// Copy already-decoded content without passing it through a shell.
    pub fn copy_bytes(&self, bytes: &[u8]) -> Result<(), ProviderError> {
        self.run(&self.wl_copy, "copy filtered text", &[], Some(bytes))
            .map(|_| ())
    }

    /// Copy an externally configured pin without invoking `cliphist`. The
    /// bound matches the maximum pin size accepted by this adapter.
    pub fn copy_pinned(&self, text: &str) -> Result<(), ProviderError> {
        let bounded: String = text.chars().take(MAX_PIN_CHARS).collect();
        self.run(
            &self.wl_copy,
            "copy pinned text",
            &[],
            Some(bounded.as_bytes()),
        )
        .map(|_| ())
    }

    /// Delete the exact row from `cliphist list`, matching QML's stdin-based
    /// `cliphist delete` protocol. The row must originate from this provider.
    pub fn delete(&self, entry: &HistoryEntry) -> Result<(), ProviderError> {
        validate_id(&entry.id)?;
        let (row_id, _) =
            entry
                .source_row
                .split_once('\t')
                .ok_or_else(|| ProviderError::Parse {
                    line: 0,
                    detail: "delete source row is malformed".into(),
                })?;
        if row_id != entry.id {
            return Err(ProviderError::Parse {
                line: 0,
                detail: "delete row id does not match entry id".into(),
            });
        }
        let mut input = entry.source_row.as_bytes().to_vec();
        input.push(b'\n');
        self.run(&self.cliphist, "delete", &[], Some(&input))
            .map(|_| ())
    }

    pub fn wipe(&self) -> Result<(), ProviderError> {
        self.run(&self.cliphist, "wipe", &[], None).map(|_| ())
    }

    /// Decode and return content for external pin persistence. Markup cleanup
    /// remains the caller/config layer's responsibility, as in QML.
    pub fn decode_for_pin(&self, id: &str) -> Result<String, ProviderError> {
        validate_id(id)?;
        let output = self.run(&self.cliphist, "decode", &[id], None)?;
        let decoded = String::from_utf8_lossy(&output.stdout);
        let text: String = decoded.chars().take(MAX_PIN_CHARS).collect();
        Ok(text)
    }

    fn run(
        &self,
        executable: &PathBuf,
        operation: &'static str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<std::process::Output, ProviderError> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().map_err(|e| launch_error(executable, e))?;
        if let Some(input) = stdin {
            let write_result = child
                .stdin
                .take()
                .expect("stdin was configured")
                .write_all(input);
            if let Err(error) = write_result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(command_error(
                    executable,
                    operation,
                    None,
                    error.to_string(),
                ));
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|e| command_error(executable, operation, None, e.to_string()))?;
        if !output.status.success() {
            return Err(command_error(
                executable,
                operation,
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(output)
    }

    fn spawn_stdout(
        &self,
        executable: &PathBuf,
        operation: &'static str,
        args: &[&str],
    ) -> Result<std::process::Child, ProviderError> {
        Command::new(executable)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| launch_error(executable, e))
            .map_err(|error| match error {
                ProviderError::Unavailable { .. } => error,
                _ => command_error(
                    executable,
                    operation,
                    None,
                    "could not start process".into(),
                ),
            })
    }
}

fn validate_id(id: &str) -> Result<(), ProviderError> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ProviderError::Parse {
            line: 0,
            detail: "entry id must contain decimal digits".into(),
        });
    }
    Ok(())
}

fn launch_error(path: &PathBuf, error: std::io::Error) -> ProviderError {
    if error.kind() == io::ErrorKind::NotFound {
        ProviderError::Unavailable {
            executable: path.clone(),
            detail: error.to_string(),
        }
    } else {
        command_error(path, "launch", None, error.to_string())
    }
}

fn command_error(
    path: &PathBuf,
    operation: &'static str,
    status: Option<i32>,
    detail: String,
) -> ProviderError {
    ProviderError::Command {
        executable: path.clone(),
        operation,
        status,
        detail,
    }
}

fn read_child_stderr(child: &mut std::process::ChildStderr) -> String {
    let mut bytes = Vec::new();
    let _ = child.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

fn sanitize_preview(preview: &str) -> String {
    let flattened = preview.split_whitespace().collect::<Vec<_>>().join(" ");
    flattened.chars().take(MAX_PREVIEW_CHARS).collect()
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn preview_matches_pin(preview: &str, pin: &str) -> bool {
    let preview = normalized(preview);
    let pin = normalized(pin);
    if preview.is_empty() || pin.is_empty() {
        return false;
    }
    if preview.chars().count() >= PREVIEW_LIMIT {
        pin.starts_with(&preview)
    } else {
        preview == pin
    }
}

fn image_placeholder(preview: &str) -> bool {
    let lower = preview.to_ascii_lowercase();
    let Some(start) = lower.find("[[") else {
        return false;
    };
    let Some(end) = lower.rfind("]]") else {
        return false;
    };
    if end <= start + 2 {
        return false;
    }
    let body = &lower[start + 2..end];
    if !body.contains("binary data") {
        return false;
    }
    body.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .count()
        >= 2
}
