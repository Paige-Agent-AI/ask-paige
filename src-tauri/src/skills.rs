//! Bundled chat guidance. Skill selection supplies context to the existing
//! Ask runner and has no effect on tool access or approval.

pub(crate) const UI_DESIGN_STANDARD: &str = include_str!("../../docs/UI-DESIGN.md");

#[derive(serde::Deserialize)]
pub(crate) struct ComposerSkill {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) guidance: String,
    #[serde(default)]
    pub(crate) design: bool,
    pub(crate) source: Option<String>,
}

pub(crate) fn composer_skill(id: &str) -> Result<ComposerSkill, String> {
    serde_json::from_str::<Vec<ComposerSkill>>(include_str!("../../src/skills.json"))
        .map_err(|_| "Could not load the built-in skills.")?
        .into_iter().find(|skill| skill.id == id)
        .ok_or_else(|| "That skill is not available in PAIGE Desktop.".into())
}

pub(crate) fn resolve(app: &tauri::AppHandle, id: &str) -> Result<ComposerSkill, String> {
    if id.starts_with("github-") {
        crate::github_skills::installed_skill(app, id)
    } else {
        composer_skill(id)
    }
}

pub(crate) fn skills_for_turn(
    selected: Option<ComposerSkill>, message: &str, visual_qa: bool,
) -> Result<Vec<(ComposerSkill, &'static str)>, String> {
    let explicit = selected;
    let selected_design = explicit.as_ref().is_some_and(|skill| skill.design);
    let mut skills: Vec<_> = explicit.into_iter().map(|skill| (skill, "manual")).collect();
    if !selected_design && let Some(id) = design_skill_for_request(message, visual_qa) {
        skills.push((composer_skill(id)?, "automatic"));
    }
    Ok(skills)
}

fn design_skill_for_request(message: &str, visual_qa: bool) -> Option<&'static str> {
    if message.trim_start().starts_with('/') { return visual_qa.then_some("game-ui-ux"); }
    let words = message.split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty()).map(str::to_lowercase).collect::<Vec<_>>();
    let text = format!(" {} ", words.join(" "));
    let includes = |phrases: &[&str]| phrases.iter().any(|phrase| text.contains(&format!(" {phrase} ")));
    let interface = visual_qa || includes(&[
        "ui", "ux", "interface", "screen", "layout", "sidebar", "workbench", "composer", "dashboard",
        "popup", "popover", "modal", "dialog", "dropdown", "toast", "button", "menu", "settings",
        "keyboard focus", "window fit", "windows scaling", "design system", "typography", "website",
        "loading state", "empty state", "visual hierarchy", "reduced motion", "prefers reduced motion",
    ]);
    let journey = includes(&[
        "onboarding", "first run", "sign up flow", "signup flow", "checkout flow", "approval journey",
        "user journey", "navigation flow", "step by step flow", "backtracking",
    ]);
    let design_action = includes(&[
        "design", "redesign", "build", "create", "improve", "fix", "adjust", "refine", "polish", "rework",
        "review", "audit", "inspect", "critique", "evaluate", "prototype", "animate", "add", "change",
        "remove", "map", "plan", "simplify", "make", "tighten", "smooth",
    ]);
    if !visual_qa && (!design_action || !(interface || journey)) { return None; }

    if (interface || journey) && includes(&[
        "animate", "animation", "animations", "motion", "transition", "transitions", "easing",
        "microinteraction", "micro interaction",
    ]) {
        Some("animate")
    } else if journey {
        Some("level-design")
    } else {
        Some("game-ui-ux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyday_design_requests_apply_one_relevant_skill() {
        let cases = [
            ("Make the Settings screen easier to scan and improve keyboard focus.", "game-ui-ux"),
            ("Redesign our first-run onboarding so people can go back a step.", "level-design"),
            ("Animate the approval toast and account for reduced motion.", "animate"),
            ("Animate onboarding between steps.", "animate"),
            ("Create a responsive website for our team.", "game-ui-ux"),
        ];
        for (request, id) in cases {
            let skills = skills_for_turn(None, request, false).unwrap();
            assert_eq!(skills.len(), 1, "{request}");
            assert_eq!(skills[0].0.id, id);
            assert!(skills[0].0.design && skills[0].0.source.is_some());
            assert_eq!(skills[0].1, "automatic");
        }
        assert!(skills_for_turn(None, "Fix the database migration.", false).unwrap().is_empty());
        assert!(skills_for_turn(None, "Which screen is open?", false).unwrap().is_empty());
        assert!(skills_for_turn(None, "/linear ANT-147", false).unwrap().is_empty());
    }

    #[test]
    fn visual_qa_and_manual_selection_use_the_same_catalog() {
        let visual = skills_for_turn(None, "Look at this screenshot and propose a fix.", true).unwrap();
        assert_eq!(visual[0].0.id, "game-ui-ux");
        let visual_with_motion = skills_for_turn(Some(composer_skill("animate").unwrap()), "Look at this screenshot and propose a fix.", true).unwrap();
        assert_eq!(visual_with_motion.len(), 1);
        assert_eq!(visual_with_motion[0].0.id, "animate");
        assert_eq!(visual_with_motion[0].1, "manual");
        let explicit = skills_for_turn(Some(composer_skill("level-design").unwrap()), "Review this modal animation.", false).unwrap();
        assert_eq!(explicit.len(), 1);
        assert_eq!(explicit[0].0.id, "level-design");
        assert_eq!(explicit[0].1, "manual");
        let with_review = skills_for_turn(Some(composer_skill("code-review").unwrap()), "Review the Settings screen layout.", false).unwrap();
        assert_eq!(with_review.iter().filter(|(skill, _)| skill.design).count(), 1);
        assert!(composer_skill("../SKILL.md").is_err());
    }
}
