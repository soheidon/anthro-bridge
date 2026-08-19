use anthro_bridge_mcp_server::mcp::{build_system_prompt, build_user_prompt};

#[test]
fn system_prompt_sets_planner_role() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("implementation planner"));
    assert!(prompt.contains("not an implementation agent"));
}

#[test]
fn system_prompt_forbids_fabrication() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("Never claim that a change has already been made"));
    assert!(prompt.contains("Never invent or fabricate test results"));
}

#[test]
fn system_prompt_forbids_assuming_repository_access() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("Never assume access to the repository"));
}

#[test]
fn user_prompt_includes_task_and_context() {
    let prompt = build_user_prompt(
        "add a save button",
        "SaveButton.tsx is a React component",
        None,
    );
    assert!(prompt.contains("add a save button"));
    assert!(prompt.contains("SaveButton.tsx"));
    assert!(!prompt.contains("## Constraints"));
}

#[test]
fn user_prompt_includes_constraints_when_present() {
    let prompt = build_user_prompt("task", "context", Some("do not commit"));
    assert!(prompt.contains("do not commit"));
    assert!(prompt.contains("## Constraints"));
}

#[test]
fn user_prompt_omits_blank_constraints() {
    let prompt = build_user_prompt("task", "context", Some("   "));
    assert!(!prompt.contains("## Constraints"));
}
