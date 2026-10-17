// SPDX-License-Identifier: AGPL-3.0-or-later
//! A terminal review of already-approved projects. Confirmation prepares metadata
//! only; authentication remains the existing browser/Keychain login workflow.
use super::{CompanyArgs, credentials::CredentialStore};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(in crate::commands) struct SelectArgs {
    #[command(flatten)]
    company: CompanyArgs,
    /// Absolute target with an existing parent; existing repositories are preserved.
    #[arg(long)]
    workspace: PathBuf,
    /// Absolute metadata journal outside the workspace, with an existing parent.
    #[arg(long)]
    journal: PathBuf,
}

#[cfg(not(feature = "tui"))]
pub(super) async fn run(_: &SelectArgs, _: &impl CredentialStore) -> Result<()> {
    anyhow::bail!("projects select requires a build with tui support; no journal was prepared")
}

#[cfg(feature = "tui")]
pub(super) async fn run(args: &SelectArgs, store: &impl CredentialStore) -> Result<()> {
    use super::{CompanyOrigin, transport::CompanyClient, workspace};
    use std::io::IsTerminal;

    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        "projects select requires an interactive terminal; use projects prepare for an explicit non-interactive review"
    );
    workspace::validate_selection_paths(&args.workspace, &args.journal)?;
    let origin = CompanyOrigin::parse(&args.company.company, args.company.allow_loopback)?;
    let enrollment = workspace::active(store, &origin)?;
    let remote = CompanyClient::new(origin.clone())?;
    let projects = remote.projects(&enrollment).await?.projects;
    let mut picker = Picker::new(projects)?;
    let disclosure = Disclosure {
        company: &origin.0,
        device_id: enrollment.device_id,
        workspace: &args.workspace,
        journal: &args.journal,
        templates: workspace::reviewed_template_preview()?,
    };
    let Some(manifest) = terminal::choose(&mut picker, &disclosure)? else {
        println!("Selection cancelled. No metadata journal, repository or settings were changed.");
        return Ok(());
    };
    // The terminal is restored before network revalidation or journal output.
    let prepare = workspace::PrepareArgs {
        company: CompanyArgs {
            company: origin.0.clone(),
            allow_loopback: args.company.allow_loopback,
        },
        project: manifest.project_id,
        area: manifest.entry.area.as_str().into(),
        owner_instance: manifest.entry.owner_instance_id,
        revision: manifest.revision,
        workspace: args.workspace.clone(),
        journal: args.journal.clone(),
    };
    let digest =
        workspace::prepare_reviewed(&prepare, &manifest, enrollment.device_id, store).await?;
    println!(
        "Prepared metadata only. No clone, tool installation, service activation, MCP registration or model call was performed."
    );
    println!(
        "A separate projects apply needs --journal pointing to this journal, --plan-sha256 {digest}, --confirm and the same --company{}.",
        if args.company.allow_loopback {
            " --allow-loopback"
        } else {
            ""
        }
    );
    Ok(())
}

#[cfg(any(feature = "tui", test))]
use super::workspace;
#[cfg(any(feature = "tui", test))]
use codifix_record::{AuthRole, OnboardingArea, OnboardingDeviceReviewProject, OnboardingProject};

#[cfg(any(feature = "tui", test))]
enum Input {
    Up,
    Down,
    Review,
    Consent,
    Confirm,
    Back,
    Cancel,
    PageUp,
    PageDown,
}
#[cfg(any(feature = "tui", test))]
enum Decision {
    Continue,
    Cancelled,
    Confirmed(Box<OnboardingProject>),
}

#[cfg(any(feature = "tui", test))]
struct Picker {
    projects: Vec<OnboardingDeviceReviewProject>,
    selected: usize,
    reviewing: bool,
    consent: bool,
    scroll: u16,
}
#[cfg(any(feature = "tui", test))]
impl Picker {
    fn new(projects: Vec<OnboardingDeviceReviewProject>) -> Result<Self> {
        // Bound both disclosure work and an invalid/ambiguous server inventory.
        anyhow::ensure!(
            projects.len() <= 1024,
            "project catalog is too large for this picker"
        );
        let mut identities = std::collections::HashSet::new();
        for project in &projects {
            anyhow::ensure!(
                identities.insert((
                    project.manifest.entry.area.as_str().to_owned(),
                    project.manifest.entry.owner_instance_id,
                    project.manifest.project_id,
                )),
                "project catalog contains duplicate area/owner/project identities"
            );
        }
        Ok(Self {
            projects,
            selected: 0,
            reviewing: false,
            consent: false,
            scroll: 0,
        })
    }

    fn current(&self) -> Option<&OnboardingDeviceReviewProject> {
        self.projects.get(self.selected)
    }

    fn unavailable(project: &OnboardingDeviceReviewProject) -> Option<&'static str> {
        if project.manifest.entry.area == OnboardingArea::Trust {
            Some("Trust is remote-only")
        } else if !project.allowed_roles.contains(&AuthRole::Reader) {
            Some("Reader scope is unavailable")
        } else if !project.offline_available || !project.manifest.entry.offline_allowed {
            Some("offline preparation is unavailable")
        } else if workspace::validate_project(project).is_err() {
            Some("reviewed manifest is invalid for local preparation")
        } else {
            None
        }
    }

    fn input(&mut self, input: Input) -> Decision {
        match input {
            Input::Cancel => return Decision::Cancelled,
            Input::Back if self.reviewing => {
                self.reviewing = false;
                self.consent = false;
                self.scroll = 0;
            }
            Input::Back => return Decision::Cancelled,
            Input::Up if !self.reviewing => self.selected = self.selected.saturating_sub(1),
            Input::Down if !self.reviewing && !self.projects.is_empty() => {
                self.selected = (self.selected + 1).min(self.projects.len() - 1);
            }
            Input::Review if self.current().is_some() => {
                self.reviewing = true;
                self.consent = false;
                self.scroll = 0;
            }
            Input::Consent if self.reviewing => {
                if self
                    .current()
                    .is_some_and(|p| Self::unavailable(p).is_none())
                {
                    self.consent = !self.consent;
                }
            }
            Input::Confirm if self.reviewing && self.consent => {
                if let Some(project) = self.current().filter(|p| Self::unavailable(p).is_none()) {
                    return Decision::Confirmed(Box::new(project.manifest.clone()));
                }
            }
            Input::Up | Input::PageUp if self.reviewing => {
                self.scroll = self
                    .scroll
                    .saturating_sub(if matches!(input, Input::PageUp) {
                        10
                    } else {
                        1
                    });
            }
            Input::Down | Input::PageDown if self.reviewing => {
                self.scroll = self
                    .scroll
                    .saturating_add(if matches!(input, Input::PageDown) {
                        10
                    } else {
                        1
                    });
            }
            _ => {}
        }
        Decision::Continue
    }
}

#[cfg(feature = "tui")]
struct Disclosure<'a> {
    company: &'a str,
    device_id: uuid::Uuid,
    workspace: &'a std::path::Path,
    journal: &'a std::path::Path,
    templates: String,
}

#[cfg(feature = "tui")]
mod terminal {
    use super::*;
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::{
        Terminal,
        backend::CrosstermBackend,
        layout::{Constraint, Layout},
        widgets::{Block, Paragraph, Wrap},
    };

    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(
                std::io::stdout(),
                LeaveAlternateScreen,
                crossterm::cursor::Show
            );
        }
    }

    fn browse_body(picker: &Picker, height: u16) -> String {
        if picker.projects.is_empty() {
            return "No projects are currently approved for this device. No fallback to a broader catalog. Esc/q: cancel.".into();
        }
        let count = usize::from(height.saturating_sub(2) / 4).max(1);
        // Put the selection first: narrow terminals wrap identity lines, so a
        // fixed count of preceding items can otherwise hide the active choice.
        let start = picker.selected;
        picker
            .projects
            .iter()
            .enumerate()
            .skip(start)
            .take(count)
            .map(|(index, project)| {
                let manifest = &project.manifest;
                format!(
                    "{} {} / {} / revision {}\n  owner {}\n  project {}\n  {}",
                    if index == picker.selected { ">" } else { " " },
                    manifest.entry.area.as_str(),
                    manifest.project.escape_debug(),
                    manifest.revision,
                    manifest.entry.owner_instance_id,
                    manifest.project_id,
                    Picker::unavailable(project)
                        .unwrap_or("Reader / offline eligible; Enter to review")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn choose(
        picker: &mut Picker,
        disclosure: &Disclosure<'_>,
    ) -> Result<Option<OnboardingProject>> {
        enable_raw_mode()?;
        let _restore = Restore;
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
        loop {
            terminal.draw(|frame| {
                let areas = Layout::vertical([Constraint::Length(4), Constraint::Min(3), Constraint::Length(5)]).split(frame.area());
                frame.render_widget(Paragraph::new(format!("Company: {}\nApproved device: {}\nOne-project metadata preparation only; no changes until explicit confirmation.", disclosure.company, disclosure.device_id)).block(Block::bordered().title("Approved project selection — not device login")), areas[0]);
                let body = if picker.reviewing {
                    let project = picker.current().expect("review requires a current project");
                    format!("Reader only. Offline copies may persist after revocation; no current-authority claim.\nWorkspace: {}\nMetadata journal: {}\n\nFULL REVIEWED MANIFEST (repository/branch, tool pins, model/output destinations):\n{}\n\nMEMORY PROJECT TEMPLATES (only missing files; custom Codex/Claude settings preserved):\n{}\n\nOnly memory-skills archives have an apply installer; other formats explicitly block apply. No generic executable plugins are installed.\nSeparate Codex/OpenAI and Claude/Anthropic MCP identities are not provisioned by this journal. No model or destination is contacted.\nTrust is remote-only. No clone, download, database, service or sync is performed here.", disclosure.workspace.display(), disclosure.journal.display(), serde_json::to_string_pretty(&project.manifest).unwrap_or_else(|_| "Manifest could not be rendered; cancel selection".into()), disclosure.templates)
                } else {
                    browse_body(picker, areas[1].height)
                };
                frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }).scroll((picker.scroll, 0)).block(Block::bordered().title(if picker.reviewing { "Full disclosure — arrows / PgUp / PgDn scroll" } else { "Current approved area + owner + project identities" })), areas[1]);
                let footer = if picker.reviewing {
                    format!("{}\n[{}] I consent to Reader/offline preparation and retention of this exact project.\nSpace: toggle consent • C: CONFIRM metadata preparation\nEsc: back / clear consent • q/Ctrl-C: cancel without preparation", picker.current().and_then(Picker::unavailable).unwrap_or("Review the complete manifest and output paths before confirming."), if picker.consent { "x" } else { " " })
                } else {
                    "Up/Down: select exact identity • Enter: full review\nTrust / unavailable offline projects are disabled; no privilege expansion.\nEsc/q/Ctrl-C: cancel without preparing metadata".into()
                };
                frame.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), areas[2]);
            })?;
            if !event::poll(std::time::Duration::from_millis(100))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let input = match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Input::Cancel
                }
                KeyCode::Char('q') => Input::Cancel,
                KeyCode::Esc => Input::Back,
                KeyCode::Up => Input::Up,
                KeyCode::Down => Input::Down,
                KeyCode::PageUp => Input::PageUp,
                KeyCode::PageDown => Input::PageDown,
                KeyCode::Enter => Input::Review,
                KeyCode::Char(' ') => Input::Consent,
                KeyCode::Char('c' | 'C') => Input::Confirm,
                _ => continue,
            };
            match picker.input(input) {
                Decision::Continue => {}
                Decision::Cancelled => return Ok(None),
                Decision::Confirmed(manifest) => return Ok(Some(*manifest)),
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn picker_short_terminal_render_keeps_selected_identity_visible_after_down() {
        use ratatui::backend::TestBackend;
        for (width, height) in [(90, 16), (90, 20), (40, 24), (40, 16)] {
            let mut projects = vec![];
            for index in 0..8 {
                let mut project = super::tests::project();
                project.manifest.project = format!("project-{index}");
                projects.push(project);
            }
            let mut picker = Picker::new(projects).unwrap();
            for _ in 0..6 {
                picker.input(Input::Down);
            }
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    let areas = Layout::vertical([
                        Constraint::Length(4),
                        Constraint::Min(3),
                        Constraint::Length(5),
                    ])
                    .split(frame.area());
                    frame.render_widget(
                        Paragraph::new(browse_body(&picker, areas[1].height))
                            .wrap(Wrap { trim: false })
                            .block(Block::bordered()),
                        areas[1],
                    );
                })
                .unwrap();
            let rendered: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                rendered.contains("> workshop / project-6"),
                "selected project hidden in {width}x{height} terminal"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn project() -> OnboardingDeviceReviewProject {
        OnboardingDeviceReviewProject {
            curated_contributions_allowed: false,
            manifest: OnboardingProject {
                project_id: uuid::Uuid::new_v4(),
                project: "same-display-name".into(),
                revision: 1,
                reviewed_by: uuid::Uuid::new_v4(),
                reviewed_at: chrono::Utc::now(),
                entry: codifix_record::OnboardingProjectEntry {
                    area: OnboardingArea::Workshop,
                    owner_instance_id: uuid::Uuid::new_v4(),
                    repository_url: "https://git.example.test/project.git".into(),
                    default_branch: "main".into(),
                    offline_allowed: true,
                    tools: vec![],
                    destinations: vec![],
                },
            },
            allowed_roles: vec![AuthRole::Reader],
            offline_available: true,
        }
    }

    #[test]
    fn picker_requires_full_review_reader_offline_consent_and_confirmation() {
        let expected = project();
        let mut picker = Picker::new(vec![expected.clone()]).unwrap();
        assert!(matches!(picker.input(Input::Consent), Decision::Continue));
        assert!(matches!(picker.input(Input::Confirm), Decision::Continue));
        picker.input(Input::Review);
        assert!(matches!(picker.input(Input::Confirm), Decision::Continue));
        picker.input(Input::Consent);
        let Decision::Confirmed(selected) = picker.input(Input::Confirm) else {
            panic!("expected explicit confirmation")
        };
        assert_eq!(selected.project_id, expected.manifest.project_id);
        assert_eq!(
            selected.entry.owner_instance_id,
            expected.manifest.entry.owner_instance_id
        );
    }

    #[test]
    fn picker_cancel_and_back_clear_consent_without_selection() {
        let mut picker = Picker::new(vec![project()]).unwrap();
        picker.input(Input::Review);
        picker.input(Input::Consent);
        picker.input(Input::Back);
        assert!(!picker.consent);
        picker.input(Input::Review);
        assert!(matches!(picker.input(Input::Confirm), Decision::Continue));
        assert!(matches!(picker.input(Input::Cancel), Decision::Cancelled));
    }

    #[test]
    fn picker_trust_missing_offline_and_writer_only_are_disabled() {
        for unavailable in 0..3 {
            let mut project = project();
            match unavailable {
                0 => project.manifest.entry.area = OnboardingArea::Trust,
                1 => project.offline_available = false,
                _ => project.allowed_roles = vec![AuthRole::Writer],
            }
            let mut picker = Picker::new(vec![project]).unwrap();
            picker.input(Input::Review);
            picker.input(Input::Consent);
            assert!(!picker.consent);
            assert!(matches!(picker.input(Input::Confirm), Decision::Continue));
        }
    }

    #[test]
    fn picker_same_name_and_uuid_across_owners_keep_complete_identity() {
        let first = project();
        let mut second = first.clone();
        second.manifest.entry.owner_instance_id = uuid::Uuid::new_v4();
        let mut picker = Picker::new(vec![first, second.clone()]).unwrap();
        picker.input(Input::Down);
        picker.input(Input::Review);
        picker.input(Input::Consent);
        let Decision::Confirmed(selected) = picker.input(Input::Confirm) else {
            panic!("expected second owner selection")
        };
        assert_eq!(
            selected.entry.owner_instance_id,
            second.manifest.entry.owner_instance_id
        );
        assert_eq!(selected.project_id, second.manifest.project_id);
    }

    #[test]
    fn picker_duplicate_composite_identity_is_rejected_and_empty_can_cancel() {
        let first = project();
        assert!(Picker::new(vec![first.clone(), first]).is_err());
        let mut picker = Picker::new(vec![]).unwrap();
        picker.input(Input::Down);
        picker.input(Input::Review);
        assert!(matches!(picker.input(Input::Confirm), Decision::Continue));
        assert!(matches!(picker.input(Input::Back), Decision::Cancelled));
    }

    #[test]
    fn picker_scrolling_does_not_consent_and_review_of_another_project_resets_it() {
        let mut picker = Picker::new(vec![project(), project()]).unwrap();
        picker.input(Input::Review);
        picker.input(Input::PageDown);
        assert_eq!(picker.scroll, 10);
        picker.input(Input::Up);
        assert_eq!(picker.scroll, 9);
        picker.input(Input::PageUp);
        assert_eq!(picker.scroll, 0);
        assert!(!picker.consent);
        picker.input(Input::Consent);
        picker.input(Input::Back);
        picker.input(Input::Down);
        picker.input(Input::Review);
        assert!(!picker.consent);
    }

    #[test]
    fn picker_cli_requires_company_and_explicit_output_paths() {
        use crate::commands::runtime::Cli;
        use clap::Parser;
        assert!(
            Cli::try_parse_from([
                "memory",
                "projects",
                "select",
                "--company",
                "https://example.test"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "memory",
                "projects",
                "select",
                "--company",
                "https://example.test",
                "--workspace",
                "/tmp/repo",
                "--journal",
                "/tmp/plan.json"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from([
                "memory",
                "projects",
                "select",
                "--company",
                "https://example.test",
                "--workspace",
                "/tmp/repo",
                "--journal",
                "/tmp/plan.json",
                "--confirm"
            ])
            .is_err()
        );
    }
}
