use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const HISTORY_LIMIT: usize = 500;
const MAX_COUNT: u64 = 9_007_199_254_740_991;
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunHistoryEntry {
    pub command: String,
    #[serde(rename = "inTerminal")]
    pub in_terminal: bool,
    pub count: u64,
}

pub(crate) fn history_path() -> Option<PathBuf> {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(state_home);
        if path.is_absolute() {
            return Some(path.join("palobar/run-command-history.json"));
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state/palobar/run-command-history.json"))
}

fn normalize_history(entries: Vec<RunHistoryEntry>) -> Vec<RunHistoryEntry> {
    let mut commands = HashSet::new();
    entries
        .into_iter()
        .filter_map(|mut entry| {
            entry.command = entry.command.trim().to_string();
            (entry.count > 0 && !entry.command.is_empty() && commands.insert(entry.command.clone()))
                .then_some(entry)
        })
        .take(HISTORY_LIMIT)
        .collect()
}

fn parse_history(contents: &[u8]) -> Result<Vec<RunHistoryEntry>, serde_json::Error> {
    let values = serde_json::from_slice::<Vec<serde_json::Value>>(contents)?;
    let entries = values
        .into_iter()
        .filter_map(|value| {
            let object = value.as_object()?;
            let command = object.get("command")?.as_str()?;
            let in_terminal = object.get("inTerminal")?.as_bool()?;
            let count = object.get("count")?.as_u64()?;
            (count > 0 && count <= MAX_COUNT).then(|| RunHistoryEntry {
                command: command.to_string(),
                in_terminal,
                count,
            })
        })
        .collect();
    Ok(normalize_history(entries))
}

pub fn load_history() -> Vec<RunHistoryEntry> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    match fs::read(path) {
        Ok(contents) => match parse_history(&contents) {
            Ok(entries) => normalize_history(entries),
            Err(error) => {
                eprintln!("[run-command-rs] could not parse command history: {error}");
                Vec::new()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            eprintln!("[run-command-rs] could not read command history: {error}");
            Vec::new()
        }
    }
}

pub fn matching_history(history: &[RunHistoryEntry], draft: &str) -> Vec<RunHistoryEntry> {
    let prefix = draft.trim_start();
    history
        .iter()
        .filter(|entry| entry.command.starts_with(prefix))
        .cloned()
        .collect()
}

pub fn record_run(command: &str, in_terminal: bool) {
    let Some(path) = history_path() else {
        eprintln!("[run-command-rs] could not determine the state directory");
        return;
    };
    let Ok(_guard) = WRITE_LOCK.lock() else {
        eprintln!("[run-command-rs] command history lock is poisoned");
        return;
    };

    let Some(directory) = path.parent() else {
        return;
    };
    if let Err(error) = DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
    {
        eprintln!("[run-command-rs] could not create history directory: {error}");
        return;
    }

    let mut entries = load_history();
    let command = command.trim();
    let count = entries
        .iter()
        .find(|entry| entry.command == command)
        .map_or(1, |entry| entry.count.saturating_add(1).min(MAX_COUNT));
    entries.retain(|entry| entry.command != command);
    entries.insert(
        0,
        RunHistoryEntry {
            command: command.to_string(),
            in_terminal,
            count,
        },
    );
    entries.truncate(HISTORY_LIMIT);

    let Ok(contents) = serde_json::to_vec_pretty(&entries) else {
        return;
    };
    let mut temporary_path = path.clone();
    let mut temporary_file = None;
    for _ in 0..10 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        temporary_path.set_extension(format!("tmp-{}-{sequence}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary_path)
        {
            Ok(file) => {
                temporary_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                eprintln!("[run-command-rs] could not create history file: {error}");
                return;
            }
        }
    }
    let Some(mut file) = temporary_file else {
        eprintln!("[run-command-rs] could not allocate a temporary history file");
        return;
    };

    let write_result = file.write_all(&contents).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        eprintln!("[run-command-rs] could not write command history: {error}");
        return;
    }
    if let Err(error) = fs::rename(&temporary_path, &path) {
        let _ = fs::remove_file(&temporary_path);
        eprintln!("[run-command-rs] could not save command history: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        matching_history, normalize_history, parse_history, RunHistoryEntry, HISTORY_LIMIT,
    };

    fn entry(command: &str, count: u64) -> RunHistoryEntry {
        RunHistoryEntry {
            command: command.to_string(),
            in_terminal: false,
            count,
        }
    }

    #[test]
    fn history_matching_preserves_recency_and_trims_only_the_left() {
        let history = vec![
            entry("git status", 1),
            entry("git log", 1),
            entry("cargo test", 1),
        ];
        let matched = matching_history(&history, "  git");
        assert_eq!(matched, vec![entry("git status", 1), entry("git log", 1)]);
    }

    #[test]
    fn history_normalization_deduplicates_and_enforces_the_limit() {
        let entries = vec![entry(" first ", 2), entry("first", 8), entry("", 1)];
        assert_eq!(normalize_history(entries), vec![entry("first", 2)]);
        assert_eq!(normalize_history(vec![entry("invalid", 0)]), Vec::new());

        let entries = (0..HISTORY_LIMIT + 1)
            .map(|index| entry(&format!("command-{index}"), 1))
            .collect();
        assert_eq!(normalize_history(entries).len(), HISTORY_LIMIT);
    }

    #[test]
    fn history_loading_skips_invalid_entries_and_keeps_terminal_mode() {
        let contents = br#"[
            {"command":"git status","inTerminal":true,"count":3},
            {"command":"invalid","inTerminal":false,"count":0},
            {"command":4,"inTerminal":false,"count":1}
        ]"#;
        assert_eq!(
            parse_history(contents).unwrap(),
            vec![RunHistoryEntry {
                command: "git status".to_string(),
                in_terminal: true,
                count: 3,
            }]
        );
    }
}
