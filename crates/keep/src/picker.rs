//! The flat workspace picker.
//!
//! One list, no hierarchy: every workspace is a peer, you type to narrow and
//! press enter. There is no notion of windows or panes to navigate through.

use std::io::Write;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, terminal};
use keep_proto::WorkspaceInfo;

pub enum Choice {
    Workspace(String),
    /// The query did not match anything, so it becomes a new workspace name.
    New(String),
    Cancelled,
}

/// Subsequence match, the cheap half of what fzf does. Returns a score where
/// tighter, earlier matches rank higher.
fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let mut score = 0i32;
    let mut pos = 0usize;
    let mut last_hit: Option<usize> = None;

    for want in needle.to_lowercase().chars() {
        let found = hay[pos..].iter().position(|c| *c == want)? + pos;
        // Reward adjacency, penalise distance from the start.
        if let Some(prev) = last_hit {
            score += if found == prev + 1 { 10 } else { -((found - prev) as i32) };
        } else {
            score -= found as i32;
        }
        last_hit = Some(found);
        pos = found + 1;
    }
    Some(score)
}

fn filtered<'a>(workspaces: &'a [WorkspaceInfo], query: &str) -> Vec<&'a WorkspaceInfo> {
    let mut scored: Vec<(i32, &WorkspaceInfo)> = workspaces
        .iter()
        .filter_map(|s| fuzzy_score(query, &s.name).map(|sc| (sc, s)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().map(|(_, s)| s).collect()
}

pub fn pick(workspaces: &[WorkspaceInfo]) -> Result<Choice> {
    let mut stdout = std::io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let result = run(&mut stdout, workspaces);

    execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run(stdout: &mut std::io::Stdout, workspaces: &[WorkspaceInfo]) -> Result<Choice> {
    let mut query = String::new();
    let mut selected = 0usize;

    loop {
        let matches = filtered(workspaces, &query);
        selected = selected.min(matches.len().saturating_sub(1));

        let (_, rows) = terminal::size().unwrap_or((80, 24));
        let room = (rows as usize).saturating_sub(3).max(1);

        execute!(stdout, terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0))?;
        writeln!(stdout, "\r  keep \x1b[90m—\x1b[0m {}\x1b[7m \x1b[0m", query)?;
        writeln!(stdout, "\r  \x1b[90m{}\x1b[0m", "─".repeat(40))?;

        if matches.is_empty() {
            let label = if query.is_empty() {
                "no workspaces yet — type a name to start one".to_string()
            } else {
                format!("enter to start a new workspace named \"{query}\"")
            };
            writeln!(stdout, "\r  \x1b[90m{label}\x1b[0m")?;
        }

        for (i, s) in matches.iter().take(room).enumerate() {
            let marker = if i == selected { "\x1b[36m▌\x1b[0m" } else { " " };
            let live = s.tabs.iter().filter(|t| !t.finished).count();
            let state = if live == 0 {
                "\x1b[90mempty\x1b[0m".to_string()
            } else if s.busy() {
                "\x1b[33mrunning\x1b[0m".to_string()
            } else if s.clients() > 0 {
                format!("\x1b[32m{} attached\x1b[0m", s.clients())
            } else {
                "\x1b[90midle\x1b[0m".to_string()
            };
            let name = if i == selected {
                format!("\x1b[1m{}\x1b[0m", s.name)
            } else {
                s.name.clone()
            };
            let tabs = if live == 1 { "1 tab".into() } else { format!("{live} tabs") };
            writeln!(stdout, "\r{marker} {name}  \x1b[90m{tabs}\x1b[0m  {state}")?;
        }
        stdout.flush()?;

        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Esc => return Ok(Choice::Cancelled),
            KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Choice::Cancelled);
            }
            KeyCode::Enter => {
                let matches = filtered(workspaces, &query);
                return Ok(match matches.get(selected) {
                    Some(s) => Choice::Workspace(s.name.clone()),
                    None if !query.is_empty() => Choice::New(query),
                    None => Choice::Cancelled,
                });
            }
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => {
                let len = filtered(workspaces, &query).len();
                if selected + 1 < len {
                    selected += 1;
                }
            }
            KeyCode::Backspace => {
                query.pop();
                selected = 0;
            }
            KeyCode::Char(c) => {
                query.push(c);
                selected = 0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str) -> WorkspaceInfo {
        WorkspaceInfo {
            name: name.into(),
            tabs: vec![keep_proto::TabInfo {
                id: 1,
                cols: 80,
                rows: 24,
                clients: 0,
                finished: false,
                title: String::new(),
                busy: false,
                split_of: 0,
                split_dir: 0,
            }],
        }
    }

    #[test]
    fn empty_query_keeps_everything() {
        let all = vec![s("alpha"), s("beta")];
        assert_eq!(filtered(&all, "").len(), 2);
    }

    #[test]
    fn matches_subsequences_not_just_prefixes() {
        let all = vec![s("brokerfy-agents"), s("taskfy"), s("evolve")];
        let names: Vec<_> = filtered(&all, "bkf").iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["brokerfy-agents"], "subsequence match failed");
    }

    #[test]
    fn adjacent_matches_outrank_scattered_ones() {
        let all = vec![s("t-a-s-k"), s("task")];
        let names: Vec<_> = filtered(&all, "task").iter().map(|s| s.name.clone()).collect();
        assert_eq!(names.first().unwrap(), "task", "scattered match ranked too high");
    }

    #[test]
    fn non_matching_query_filters_everything_out() {
        let all = vec![s("alpha")];
        assert!(filtered(&all, "zzz").is_empty());
    }
}
