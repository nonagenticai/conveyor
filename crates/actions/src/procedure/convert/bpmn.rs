#![forbid(unsafe_code)]
//! The BPMN 2.0 reader behind T16's SOP ingestion (SPEC_V2 §5.6, decision D4).
//!
//! A BPMN diagram is untrusted input. It is read with `quick-xml`'s pull parser — no DOM, no
//! entity resolver — and only three things leave this module: the process container's name, the
//! activities (`kind = "computed"`) and the gateways (`kind = "gate"`). Element text is never
//! interpreted and nothing read here is executed; a broken document is an ordinary [`BpmnError`],
//! never a panic.
//!
//! Ordering is flow-first: `sequenceFlow` edges are walked breadth-first from a start event, and
//! each activity or gateway is appended once, as reached. When the graph cannot be walked in
//! full — no start event, or an element the walk never reaches — the document order of the same
//! elements is used instead. Either way the result is deterministic and each step appears once.
//!
//! Prefixes are ignored: `bpmn:task`, `bpmn2:task` and bare `task` all match, because only the
//! local name (after the last `:`) is compared.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

/// The largest document accepted, in bytes: an SOP diagram is tens of kilobytes, and a cap keeps
/// a hostile upload from making this reader the expensive part of an ingest.
const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// The `kind` of a step taken from an activity.
const ACTIVITY: &str = "computed";
/// The `kind` of a step taken from a gateway.
const GATEWAY: &str = "gate";

/// A BPMN diagram reduced to the draft a process version starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BpmnDraft {
    /// The first `<process>` element's `name` when it carries one, else its `id`.
    pub process_name: String,
    /// The activities and gateways, in flow order when the graph can be walked, else in document
    /// order — each exactly once.
    pub steps: Vec<BpmnStep>,
}

/// One draft step taken from a BPMN flow element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BpmnStep {
    /// The element's `name` attribute, else its `id`.
    pub name: String,
    /// `"computed"` for an activity, `"gate"` for a gateway.
    pub kind: &'static str,
    /// The element's `id` — the PDDL action text placeholder.
    pub action_text: String,
}

/// Why a document was not accepted as a BPMN process.
#[derive(Debug)]
pub struct BpmnError {
    /// The human-readable reason; the caller records it, so there is no error taxonomy here.
    pub detail: String,
}

impl fmt::Display for BpmnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for BpmnError {}

/// Parse a BPMN 2.0 XML document into a draft process.
///
/// `kind` on each step is `"computed"` for an activity and `"gate"` for a gateway. Refused with
/// [`BpmnError`]: input over [`MAX_DOCUMENT_BYTES`], input that is not well-formed XML, and XML
/// that carries no `<process>` element (a bare `<definitions/>` included).
pub fn parse(xml: &[u8]) -> Result<BpmnDraft, BpmnError> {
    if xml.len() > MAX_DOCUMENT_BYTES {
        return Err(BpmnError {
            detail: format!(
                "the BPMN document is {} bytes, over the {MAX_DOCUMENT_BYTES}-byte cap",
                xml.len()
            ),
        });
    }
    let mut reader = Reader::from_reader(xml);
    let mut state = State::default();
    loop {
        let event = reader.read_event().map_err(|error| BpmnError {
            detail: format!("not well-formed XML: {error}"),
        })?;
        match event {
            Event::Start(element) | Event::Empty(element) => state.visit(&element)?,
            Event::Eof => break,
            // Text, comments, declarations and DOCTYPEs carry nothing this reader needs, and
            // quick-xml never resolves an external entity on its own.
            _ => {}
        }
    }
    state.finish()
}

/// Everything the one pass over the document collects, in document order.
#[derive(Debug, Default)]
struct State {
    /// The first `<process>` element's name, once seen.
    process_name: Option<String>,
    /// Whether a `<definitions>` element was seen at all; only shapes the refusal detail.
    definitions_seen: bool,
    /// The activities and gateways, in document order.
    elements: Vec<Element>,
    /// Element id → index into `elements`; the first occurrence wins.
    index_by_id: HashMap<String, usize>,
    /// `sequenceFlow` edges: sourceRef → targetRefs, in document order.
    flows: HashMap<String, Vec<String>>,
    /// `startEvent` ids, in document order.
    start_events: Vec<String>,
}

/// One collected activity or gateway.
#[derive(Debug)]
struct Element {
    /// The `id` attribute: the action text, and the key the flow walk reaches it by.
    id: Option<String>,
    /// The `name` attribute, preferred as the step name.
    name: Option<String>,
    /// [`ACTIVITY`] or [`GATEWAY`].
    kind: &'static str,
}

impl State {
    /// Take whatever this element contributes; an unknown element contributes nothing.
    fn visit(&mut self, element: &BytesStart<'_>) -> Result<(), BpmnError> {
        let tag = element.name();
        let local = local_name(tag.as_ref());
        let attributes = attributes_of(element)?;
        match local {
            "definitions" => self.definitions_seen = true,
            "process" => {
                if self.process_name.is_none() {
                    self.process_name = Some(
                        pick(attribute(&attributes, "name"))
                            .or_else(|| pick(attribute(&attributes, "id")))
                            .unwrap_or_default()
                            .to_owned(),
                    );
                }
            }
            "startEvent" => {
                if let Some(id) = pick(attribute(&attributes, "id")) {
                    self.start_events.push(id.to_owned());
                }
            }
            "sequenceFlow" => {
                if let (Some(source), Some(target)) = (
                    pick(attribute(&attributes, "sourceRef")),
                    pick(attribute(&attributes, "targetRef")),
                ) {
                    self.flows
                        .entry(source.to_owned())
                        .or_default()
                        .push(target.to_owned());
                }
            }
            other => {
                if let Some(kind) = step_kind(other) {
                    let id = pick(attribute(&attributes, "id")).map(str::to_owned);
                    let name = pick(attribute(&attributes, "name")).map(str::to_owned);
                    if let Some(id) = &id {
                        // BPMN ids are unique; should a document repeat one, the first step
                        // keeps it and the duplicate still appears in the fallback order.
                        self.index_by_id
                            .entry(id.clone())
                            .or_insert(self.elements.len());
                    }
                    self.elements.push(Element { id, name, kind });
                }
            }
        }
        Ok(())
    }

    /// The draft, or the refusal when the document never declared a process.
    fn finish(&self) -> Result<BpmnDraft, BpmnError> {
        let Some(process_name) = &self.process_name else {
            let detail = if self.definitions_seen {
                "the document is not a BPMN process: it declares no <process> element".to_owned()
            } else {
                "the document is not BPMN: it has neither a <definitions> nor a <process> element"
                    .to_owned()
            };
            return Err(BpmnError { detail });
        };
        let order = self
            .flow_order()
            .unwrap_or_else(|| (0..self.elements.len()).collect());
        Ok(BpmnDraft {
            process_name: process_name.clone(),
            steps: order
                .into_iter()
                .map(|index| self.elements[index].step())
                .collect(),
        })
    }

    /// The flow order, when one start event reaches every activity and gateway; otherwise
    /// `None`, and the caller falls back to document order.
    fn flow_order(&self) -> Option<Vec<usize>> {
        if self.elements.is_empty() {
            return Some(Vec::new());
        }
        for start in &self.start_events {
            let reached = self.walk(start);
            if reached.len() == self.elements.len() {
                return Some(reached);
            }
        }
        None
    }

    /// A breadth-first walk over `sequenceFlow` edges from `start`. A step element is appended
    /// the first time it is reached; anchors and every other element are traversed but never
    /// appended, and a revisited id is never expanded twice, so a cycle terminates.
    fn walk(&self, start: &str) -> Vec<usize> {
        let mut order = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut appended: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        queue.push_back(start);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(index) = self.index_by_id.get(id)
                && appended.insert(*index)
            {
                order.push(*index);
            }
            if let Some(targets) = self.flows.get(id) {
                for target in targets {
                    queue.push_back(target.as_str());
                }
            }
        }
        order
    }
}

impl Element {
    /// The draft step this element becomes.
    fn step(&self) -> BpmnStep {
        BpmnStep {
            name: self
                .name
                .clone()
                .or_else(|| self.id.clone())
                .unwrap_or_default(),
            kind: self.kind,
            action_text: self.id.clone().unwrap_or_default(),
        }
    }
}

/// The element's attributes, keyed by local name. Values are normalised (the five predefined
/// entities and character references resolved) and never fetched from anywhere.
fn attributes_of(element: &BytesStart<'_>) -> Result<Vec<(String, String)>, BpmnError> {
    let mut attributes = Vec::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| BpmnError {
            detail: format!("an attribute is not valid: {error}"),
        })?;
        let key = attribute.key.as_ref();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| BpmnError {
                detail: format!("the `{key}` attribute is not valid: {error}"),
            })?;
        attributes.push((local_name(key).to_owned(), value.into_owned()));
    }
    Ok(attributes)
}

/// The value of the first attribute with this local name.
fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value.as_str())
}

/// A usable attribute value: present and not empty.
fn pick(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// The step kind an element's local name maps to, if it is an activity or a gateway.
fn step_kind(local: &str) -> Option<&'static str> {
    match local {
        "task" | "userTask" | "serviceTask" | "scriptTask" | "manualTask" | "businessRuleTask"
        | "sendTask" | "receiveTask" | "subProcess" => Some(ACTIVITY),
        "exclusiveGateway" | "parallelGateway" | "inclusiveGateway" | "eventBasedGateway"
        | "complexGateway" => Some(GATEWAY),
        _ => None,
    }
}

/// The local name of a tag or attribute: `bpmn:task` → `task`, `task` → `task`.
fn local_name(raw: &str) -> &str {
    match raw.rsplit_once(':') {
        Some((_, local)) => local,
        None => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_DOCUMENT_BYTES, parse};

    /// start → task A → exclusiveGateway → task B → end.
    const LINEAR_FLOW: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <bpmn:process id="Process_1" name="Order handling">
    <bpmn:startEvent id="start" name="Order received">
      <bpmn:outgoing>flow_1</bpmn:outgoing>
    </bpmn:startEvent>
    <bpmn:task id="task_a" name="Receive order"/>
    <bpmn:exclusiveGateway id="gate_1" name="Stock available?"/>
    <bpmn:userTask id="task_b" name="Back-order item"/>
    <bpmn:endEvent id="end" name="Done"/>
    <bpmn:sequenceFlow id="flow_1" sourceRef="start" targetRef="task_a"/>
    <bpmn:sequenceFlow id="flow_2" sourceRef="task_a" targetRef="gate_1"/>
    <bpmn:sequenceFlow id="flow_3" sourceRef="gate_1" targetRef="task_b"/>
    <bpmn:sequenceFlow id="flow_4" sourceRef="task_b" targetRef="end"/>
  </bpmn:process>
</bpmn:definitions>"#;

    #[test]
    fn the_linear_fixture_yields_activity_gateway_activity_in_flow_order() {
        let draft = parse(LINEAR_FLOW.as_bytes()).expect("the fixture is valid BPMN");
        assert_eq!(draft.process_name, "Order handling");
        let steps: Vec<(&str, &str, &str)> = draft
            .steps
            .iter()
            .map(|step| (step.name.as_str(), step.kind, step.action_text.as_str()))
            .collect();
        assert_eq!(
            steps,
            vec![
                ("Receive order", "computed", "task_a"),
                ("Stock available?", "gate", "gate_1"),
                ("Back-order item", "computed", "task_b"),
            ],
        );
    }

    /// No `startEvent` and no flows at all: the only order left is document order.
    const NO_START: &str = r#"<definitions>
  <process id="process_2">
    <task id="task_a" name="First"/>
    <exclusiveGateway id="gate_1" name="Then"/>
    <userTask id="task_b" name="Last"/>
  </process>
</definitions>"#;

    #[test]
    fn without_a_start_event_the_steps_keep_document_order() {
        let draft = parse(NO_START.as_bytes()).expect("the fixture is valid BPMN");
        assert_eq!(draft.process_name, "process_2");
        let names: Vec<&str> = draft.steps.iter().map(|step| step.name.as_str()).collect();
        assert_eq!(names, vec!["First", "Then", "Last"]);
        assert_eq!(draft.steps[1].kind, "gate");
    }

    #[test]
    fn malformed_xml_is_refused() {
        assert!(parse(b"<not-xml").is_err());
        assert!(parse(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn definitions_without_a_process_is_refused() {
        let error = parse(b"<definitions/>").expect_err("a definitions-only document is not BPMN");
        assert!(
            error.detail.contains("BPMN"),
            "the detail names the refusal: {}",
            error.detail,
        );
    }

    /// Document order disagrees with flow order: the sequence-flow walk must win. The first
    /// task also has no `name`, so its step name falls back to its `id`.
    const FLOW_DISAGREES: &str = r#"<bpmn2:definitions xmlns:bpmn2="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <bpmn2:process id="process_3">
    <bpmn2:startEvent id="start"/>
    <bpmn2:userTask id="second" name="Second"/>
    <bpmn2:task id="first"/>
    <bpmn2:sequenceFlow id="flow_1" sourceRef="start" targetRef="first"/>
    <bpmn2:sequenceFlow id="flow_2" sourceRef="first" targetRef="second"/>
  </bpmn2:process>
</bpmn2:definitions>"#;

    #[test]
    fn the_sequence_flow_walk_wins_over_document_order() {
        let draft = parse(FLOW_DISAGREES.as_bytes()).expect("the fixture is valid BPMN");
        assert_eq!(draft.process_name, "process_3");
        let steps: Vec<(&str, &str)> = draft
            .steps
            .iter()
            .map(|step| (step.name.as_str(), step.action_text.as_str()))
            .collect();
        assert_eq!(steps, vec![("first", "first"), ("Second", "second")]);
    }

    /// The walk reaches one task only, so the whole order falls back to document order — which
    /// puts the orphan first.
    const ORPHANED: &str = r#"<definitions>
  <process id="process_4">
    <startEvent id="start"/>
    <task id="orphan" name="Orphan"/>
    <task id="reachable" name="Reachable"/>
    <sequenceFlow id="flow_1" sourceRef="start" targetRef="reachable"/>
  </process>
</definitions>"#;

    #[test]
    fn an_unreachable_step_falls_back_to_document_order() {
        let draft = parse(ORPHANED.as_bytes()).expect("the fixture is valid BPMN");
        let names: Vec<&str> = draft.steps.iter().map(|step| step.name.as_str()).collect();
        assert_eq!(names, vec!["Orphan", "Reachable"]);
    }

    /// start → intermediate catch event → service task → end. The anchor and the unknown
    /// element are not steps, but the walk traverses them; the text annotation is ignored.
    const THROUGH_AN_EVENT: &str = r#"<definitions>
  <process id="process_5">
    <startEvent id="start"/>
    <intermediateCatchEvent id="wait" name="Timer"/>
    <serviceTask id="call" name="Call billing"/>
    <endEvent id="end"/>
    <textAnnotation id="note" text="not a step"/>
    <sequenceFlow id="flow_1" sourceRef="start" targetRef="wait"/>
    <sequenceFlow id="flow_2" sourceRef="wait" targetRef="call"/>
    <sequenceFlow id="flow_3" sourceRef="call" targetRef="end"/>
  </process>
</definitions>"#;

    #[test]
    fn anchors_and_unknown_elements_are_not_steps_but_the_walk_traverses_them() {
        let draft = parse(THROUGH_AN_EVENT.as_bytes()).expect("the fixture is valid BPMN");
        assert_eq!(draft.steps.len(), 1);
        assert_eq!(draft.steps[0].name, "Call billing");
        assert_eq!(draft.steps[0].kind, "computed");
        assert_eq!(draft.steps[0].action_text, "call");
    }

    #[test]
    fn a_document_over_the_cap_is_refused_before_it_is_parsed() {
        let oversized = vec![b' '; MAX_DOCUMENT_BYTES + 1];
        let error = parse(&oversized).expect_err("over the 4 MiB cap");
        assert!(
            error.detail.contains("cap"),
            "the detail names the cap: {}",
            error.detail,
        );
    }
}
