use usagi::domain::project::{ProjectState, Worktree};

#[test]
fn test_project_state_basic() {
    let mut state = ProjectState {
        version: 1,
        initialized: true,
        worktrees: vec![Worktree {
            branch: "main".to_string(),
            directory: "main".to_string(),
            default: true,
            modified_at: "".to_string(),
            status: usagi::domain::project::SessionStatus::Todo,
            has_upstream: false,
        }],
        current_worktree: Some("main".to_string()),
        last_updated: None,
        ai_model: None,
    };

    state.update_last_updated();
    assert!(state.last_updated.is_some());

    state.worktrees[0].update_modified_at();
    assert!(!state.worktrees[0].modified_at.is_empty());
}
