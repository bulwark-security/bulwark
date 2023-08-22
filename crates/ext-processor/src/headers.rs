use bulwark_wasm_sdk::{Decision, Outcome};
use sfv::{BareItem, Decimal, Dictionary, FromPrimitive, Item, List, ListEntry, SerializeValue};

// TODO: capture the entire outcome: accepted/suspicious/restricted + threshold values
// TODO: should this error for invalid Decision values?

/// Serialize a combined [`Decision`] into a [SFV](sfv) header value to be sent with the request to the interior service.
pub(crate) fn serialize_decision_sfv(
    decision: Decision,
    outcome: Outcome,
) -> std::result::Result<String, &'static str> {
    let accept_value = Item::new(BareItem::Decimal(
        Decimal::from_f64(decision.accept).unwrap(),
    ));
    let restrict_value = Item::new(BareItem::Decimal(
        Decimal::from_f64(decision.restrict).unwrap(),
    ));
    let unknown_value = Item::new(BareItem::Decimal(
        Decimal::from_f64(decision.unknown).unwrap(),
    ));
    let score_value = Item::new(BareItem::Decimal(
        Decimal::from_f64(decision.pignistic().restrict).unwrap(),
    ));
    let outcome_value = Item::new(BareItem::String(outcome.to_string()));

    let mut dict = Dictionary::new();
    dict.insert("accept".into(), accept_value.into());
    dict.insert("restrict".into(), restrict_value.into());
    dict.insert("unknown".into(), unknown_value.into());
    dict.insert("score".into(), score_value.into());
    dict.insert("outcome".into(), outcome_value.into());

    dict.serialize_value()
}

/// Serialize a tag [`Vec`] into a [SFV](sfv) header value to be sent with the request to the interior service.
pub(crate) fn serialize_tags_sfv(tags: Vec<String>) -> std::result::Result<String, &'static str> {
    let list: List = tags
        .iter()
        .map(|tag| ListEntry::from(Item::new(BareItem::Token(tag.to_string()))))
        .collect::<Vec<ListEntry>>();
    list.serialize_value()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_serialize_decision_sfv() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            serialize_decision_sfv(
                Decision {
                    accept: 0.0,
                    restrict: 0.0,
                    unknown: 1.0
                },
                Outcome::Suspected
            )?,
            "accept=0.0, restrict=0.0, unknown=1.0, score=0.5, outcome=\"suspected\""
        );
        assert_eq!(
            serialize_decision_sfv(
                Decision {
                    accept: 0.0,
                    restrict: 1.0,
                    unknown: 0.0
                },
                Outcome::Restricted
            )?,
            "accept=0.0, restrict=1.0, unknown=0.0, score=1.0, outcome=\"restricted\""
        );
        assert_eq!(
            serialize_decision_sfv(
                Decision {
                    accept: 1.0,
                    restrict: 0.0,
                    unknown: 0.0
                },
                Outcome::Accepted
            )?,
            "accept=1.0, restrict=0.0, unknown=0.0, score=0.0, outcome=\"accepted\""
        );
        assert_eq!(
            serialize_decision_sfv(
                Decision {
                    accept: 0.333,
                    restrict: 0.333,
                    unknown: 0.333
                },
                Outcome::Suspected
            )?,
            "accept=0.333, restrict=0.333, unknown=0.333, score=0.500, outcome=\"suspected\""
        );

        Ok(())
    }

    #[test]
    fn test_serialize_tags_sfv() -> Result<(), Box<dyn std::error::Error>> {
        // TODO: handling for empty lists, maybe just omit header on empty
        // assert_eq!(
        //     serialize_tags_sfv(vec![]),
        //     "accept=0.0, restrict=0.0, unknown=1.0"
        // );
        assert_eq!(serialize_tags_sfv(vec!["hello".to_string()])?, "hello");
        assert_eq!(
            serialize_tags_sfv(vec!["a".to_string(), "b".to_string(), "c".to_string()])?,
            "a, b, c"
        );
        assert_eq!(
            serialize_tags_sfv(vec!["first-item".to_string(), "second-item".to_string()])?,
            "first-item, second-item"
        );
        // TODO: handling for disallowed characters
        // assert_eq!(
        //     serialize_tags_sfv(vec!["nO@!#%!@#(;',.".to_string(), "/*-:1!_-<>".to_string()]),
        //     "first-item, second-item"
        // );

        Ok(())
    }
}
