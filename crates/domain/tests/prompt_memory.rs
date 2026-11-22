//! S21 step 3 (#15) — memory and knowledge content in the compiled prompt.
//!
//! The prompt is compiled, never stored, so these tests pin the three properties a silent change
//! would break: the selected rows and hit identities are rendered; an optional source with no
//! content is absent while a required one refuses by name; and the same recipe + facts produce
//! the same bytes and hash while a changed selection changes the hash.

use conveyor_domain::memory::{MemoryKind, MemoryLine, MemorySource};
use conveyor_domain::prompt::{
    KnowledgeFacts, KnowledgeHit, KnowledgeSelector, MemorySelector, PromptCompileError,
    PromptDefinition, PromptFacts, PromptSection, PromptSource, compile,
};
use serde_json::json;
use uuid::Uuid;

fn section(
    position: i32,
    source: PromptSource,
    required: bool,
    selector: serde_json::Value,
) -> PromptSection {
    PromptSection {
        position,
        source,
        required,
        template: if source == PromptSource::System {
            "You work carefully.".to_owned()
        } else {
            String::new()
        },
        selector,
        budget_chars: None,
    }
}

fn recipe(memory_required: bool) -> PromptDefinition {
    PromptDefinition {
        key: "agent".to_owned(),
        version: 3,
        title: "Agent".to_owned(),
        sections: vec![
            section(1, PromptSource::System, true, json!({})),
            section(
                2,
                PromptSource::Knowledge,
                false,
                json!({"query": "header"}),
            ),
            section(
                3,
                PromptSource::Memory,
                memory_required,
                json!({"limit": 8}),
            ),
            section(4, PromptSource::ProjectCard, false, json!({})),
        ],
        budget_chars: None,
    }
}

fn line(id: u128, kind: MemoryKind, body: &str) -> MemoryLine {
    MemoryLine {
        id: Uuid::from_u128(id),
        kind,
        source: MemorySource::Human,
        body: body.to_owned(),
        created_at: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).expect("epoch"),
    }
}

fn facts() -> PromptFacts {
    PromptFacts {
        knowledge: KnowledgeFacts {
            packs: vec!["pack@2".to_owned()],
            hits: vec![KnowledgeHit {
                id: "hit-1".to_owned(),
                text: "the pinned passage".to_owned(),
            }],
        },
        memory: vec![
            line(1, MemoryKind::Fact, "the header regressed before"),
            line(2, MemoryKind::Decision, "we chose a CSS fix"),
        ],
        ..PromptFacts::default()
    }
}

#[test]
fn the_selected_memory_and_knowledge_content_are_rendered_with_their_identities() {
    let compiled = compile(&recipe(false), &facts()).expect("compile");
    assert!(
        compiled
            .text
            .contains("## Knowledge\npacks: pack@2\n- hit-1: the pinned passage"),
        "knowledge content and pack identity: {}",
        compiled.text
    );
    assert!(
        compiled.text.contains(
            "## Memory\n- [fact/human#00000000-0000-0000-0000-000000000001] the header regressed before\n\
             - [decision/human#00000000-0000-0000-0000-000000000002] we chose a CSS fix"
        ),
        "memory rows in selection order with their ids: {}",
        compiled.text
    );
    // Trust order: the agent reads knowledge, then what the work learned, then the raw card.
    let knowledge = compiled.text.find("## Knowledge").expect("knowledge");
    let memory = compiled.text.find("## Memory").expect("memory");
    assert!(knowledge < memory, "knowledge precedes memory");
}

#[test]
fn an_optional_memory_section_with_no_rows_is_absent_and_a_required_one_refuses() {
    let mut facts = facts();
    facts.memory.clear();
    let compiled = compile(&recipe(false), &facts).expect("optional memory is absent");
    assert!(!compiled.text.contains("## Memory"));

    assert_eq!(
        compile(&recipe(true), &facts).expect_err("required memory"),
        PromptCompileError::MissingRequired {
            kind: PromptSource::Memory
        }
    );
}

#[test]
fn the_same_recipe_and_facts_compile_byte_identically_and_a_changed_selection_changes_the_hash() {
    let first = compile(&recipe(false), &facts()).expect("first");
    let second = compile(&recipe(false), &facts()).expect("second");
    assert_eq!(first.text, second.text);
    assert_eq!(first.sha256, second.sha256);

    let mut changed = facts();
    changed.memory[0].body = "the header regressed in a different way".to_owned();
    let third = compile(&recipe(false), &changed).expect("third");
    assert_ne!(first.sha256, third.sha256);
    assert_ne!(first.text, third.text);
}

#[test]
fn the_memory_selector_is_closed_and_bounded() {
    assert_eq!(
        MemorySelector::parse(&json!({})).expect("default"),
        MemorySelector::default()
    );
    assert_eq!(
        MemorySelector::parse(
            &json!({"kinds": ["fact", "decision"], "limit": 4, "include_digest": false})
        )
        .expect("explicit"),
        MemorySelector {
            kinds: vec![MemoryKind::Fact, MemoryKind::Decision],
            limit: 4,
            include_digest: false,
        }
    );
    assert!(MemorySelector::parse(&json!({"limit": 0})).is_err());
    assert!(MemorySelector::parse(&json!({"limit": 33})).is_err());
    assert!(MemorySelector::parse(&json!({"kinds": ["digest"]})).is_err());
    assert!(MemorySelector::parse(&json!({"unknown": true})).is_err());
}

#[test]
fn the_knowledge_selector_is_closed_and_bounded() {
    assert_eq!(
        KnowledgeSelector::parse(&json!({"query": "  header  ", "top_k": 3})).expect("explicit"),
        KnowledgeSelector {
            query: Some("header".to_owned()),
            top_k: 3,
        }
    );
    assert_eq!(
        KnowledgeSelector::parse(&json!({})).expect("default"),
        KnowledgeSelector::default()
    );
    assert!(KnowledgeSelector::parse(&json!({"top_k": 0})).is_err());
    assert!(KnowledgeSelector::parse(&json!({"query": 4})).is_err());
    assert!(KnowledgeSelector::parse(&json!({"unknown": true})).is_err());
}
