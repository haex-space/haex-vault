// Spec 087's chain set: it loads, and each request's expected chain, played on the fixture world
// through the dialog, passes, so a failure in a measurement is the model's and not the set's.
use std::path::PathBuf;

use serde_json::Value;

use super::mail_names::MailNames;
use super::runner::{app_registry, Profile};
use super::scoring::{effects, End, Outcome};
use super::test_support::{concrete_args, score, trace, with_required};
use super::{embedded_mail_tools, embedded_world, load_set_file, Expectation};
use crate::chat::eval::embedded_tools;

fn chain_set_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/087-agent-chains/analysis/chainset.json")
}

#[test]
fn every_expected_chain_of_the_chain_set_passes() {
    let set = load_set_file(&chain_set_path()).unwrap();
    let world = embedded_world();
    let mail = embedded_mail_tools();
    let names = MailNames::of(&mail);
    let effects = effects(&embedded_tools(), &mail);
    let registry = app_registry(&embedded_tools(), &mail, Profile::Desktop);
    let registry = registry.lock().unwrap();
    assert!(set.requests.len() >= 30, "{} requests", set.requests.len());
    for request in &set.requests {
        // A refusal has no chain to play; the set's refusals check that a model does not search on
        // where nothing can be found (R11.2).
        let chains = match &request.expect {
            Expectation::Calls(chains) => chains,
            Expectation::Refusal => continue,
            _ => panic!(
                "{}: a chain-set request expects calls or a refusal",
                request.id
            ),
        };
        let mut calls: Vec<(String, Value)> = Vec::new();
        let mut previous = Value::Null;
        for expected in &chains[0] {
            let tool = registry
                .get(names.in_app(&expected.tool))
                .expect("a registered tool");
            let mut args = with_required(
                concrete_args(&expected.args, &previous, &world),
                &tool.input_schema(),
            );
            // A model drafts in the account of the mail it answers.
            if args.get("account").and_then(Value::as_str) == Some("placeholder") {
                if let Some(account) = previous["messages"][0]["account"].as_str() {
                    args["account"] = Value::from(account);
                }
            }
            let answer = world.answer(&expected.tool, &args);
            previous = serde_json::from_str(&answer.content).unwrap_or(Value::Null);
            calls.push((expected.tool.clone(), args));
        }
        let calls: Vec<(&str, Value)> =
            calls.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
        let played = trace(&world, &calls, "Erledigt.", End::Answer);
        assert_eq!(
            score(request, &played, &effects, &world),
            Outcome::Pass,
            "{}: {calls:?}",
            request.id
        );
    }
}
