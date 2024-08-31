use crate::model::{Entity, EntityRule, EntityRuleType};

pub struct YarnFormatter;

/*
    Format: zk=3,NOTIN,NODE,zk:hbase=5,IN,RACK,zk
*/

impl YarnFormatter {
    pub fn new() -> Self {
        Self
    }

    fn format_rule(rule: &EntityRule) -> String {
        // let number_of_containers = rule.metadata("numberOfContainers").unwrap_or("0");
        let scope = rule.metadata("scope").unwrap_or("NODE");
        let r#type = rule.r#type();
        let op = match r#type {
            EntityRuleType::Require => "IN",
            EntityRuleType::Exclude => "NOTIN",
        };

        let targets = rule.targets();

        match targets.len() {
            0 => panic!("No targets found"),
            1 => {
                let target = targets.first().unwrap().as_ref();
                format!("{},{},{}", op, scope, target)
            }
            _ => match r#type {
                EntityRuleType::Require => {
                    let inner = targets
                        .iter()
                        .map(|target| format!("{},{},{}", op, scope, target.as_ref()))
                        .collect::<Vec<_>>()
                        .join(":");

                    format!("OR({})", inner)
                }
                EntityRuleType::Exclude => {
                    let inner = targets
                        .iter()
                        .map(|target| format!("{},{},{}", op, scope, target.as_ref()))
                        .collect::<Vec<_>>()
                        .join(":");

                    format!("AND({})", inner)
                }
            },
        }
    }

    fn format_entity(entity: &Entity) -> String {
        let mut output = String::new();

        output.push_str(entity.name.as_ref());
        output.push('=');

        let any_rule = entity.rules().next().unwrap();
        let number_of_containers = any_rule.metadata("numberOfContainer").unwrap_or("0");

        output.push_str(number_of_containers);
        output.push(',');

        let has_one_more_rules = entity.rules_len() > 1;

        match has_one_more_rules {
            false => {
                output.push_str(Self::format_rule(any_rule).as_str());
            }
            true => {
                let inner = entity
                    .rules()
                    .map(Self::format_rule)
                    .collect::<Vec<_>>()
                    .join(":");

                let inner = format!("AND({})", inner);

                output.push_str(inner.as_str());
            }
        }

        output
    }

    pub fn format(&self, entities: &[Entity]) -> String {
        let output = entities
            .iter()
            .map(Self::format_entity)
            .collect::<Vec<_>>()
            .join(":");

        output
    }
}
