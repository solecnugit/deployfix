use crate::model::EntityRule;
use annotate_snippets::{Annotation, AnnotationType, Renderer, Slice, Snippet, SourceAnnotation};

pub struct ConflictAnnotater<'a> {
    entity_name: &'a str,
    entity_source: String,
    entity_origin: String,
    rule_range: (usize, usize),
    rule_line: usize,
}

impl<'a> ConflictAnnotater<'a> {
    fn read_source(entity_rule: &'a EntityRule) -> String {
        match entity_rule.meta_file() {
            Some(file) => {
                let source = std::fs::read_to_string(file).unwrap();
                let range = entity_rule.range();

                let lines = source.lines().collect::<Vec<_>>();
                let line = entity_rule.meta_line().unwrap_or(0);
                // If the range is specified, use it
                if let Some((start, end)) = range {
                    let start_line = source[..start].matches('\n').count() - 1;
                    let end_line = source[..end].matches('\n').count() - 1;

                    let start = (start_line - 1).max(0);
                    let end = (end_line + 1).min(lines.len() - 1);

                    lines[start..=end].join("\n")
                } else if line > 0 {
                    let start = (line - 2).max(0);
                    let end = (line + 6).min(lines.len() - 1);

                    lines[start..=end].join("\n")
                } else {
                    source
                }
            }
            None => "unknown".to_string(),
        }
    }

    pub fn new(entity_name: &'a str, entity_rule: &'a EntityRule) -> ConflictAnnotater<'a> {
        let entity_source = Self::read_source(entity_rule);
        let entity_origin = entity_rule
            .meta_file()
            .or(entity_rule.file())
            .unwrap_or("unknown")
            .to_string();
        let rule_range = entity_rule.range().unwrap_or((0, 0));
        let rule_line = entity_rule.meta_line().or(entity_rule.line()).unwrap_or(0);

        ConflictAnnotater {
            entity_name,
            entity_source,
            entity_origin,
            rule_range,
            rule_line,
        }
    }

    pub fn get_entity_name(&self) -> &str {
        self.entity_name
    }

    pub fn get_source(&self) -> &str {
        self.entity_source.as_str()
    }

    pub fn annotate(&self) -> String {
        let label = format!("Unscheduable entity: {}", self.entity_name);

        let snippet = Snippet {
            title: Some(Annotation {
                id: None,
                label: Some(label.as_str()),
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: self.entity_source.as_str(),
                line_start: self.rule_line,
                origin: Some(self.entity_origin.as_str()),
                fold: false,
                annotations: vec![SourceAnnotation {
                    label: &label,
                    annotation_type: AnnotationType::Error,
                    range: self.rule_range,
                }],
            }],
        };

        let renderer = Renderer::styled();
        let output = format!("{}", renderer.render(snippet));

        output
    }
}
