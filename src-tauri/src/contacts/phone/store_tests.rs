use super::*;
use crate::contacts::service::test_support::{person, test_vault, value};
use crate::contacts::service::Caller;

async fn vault_with_anna() -> (
    crate::contacts::service::test_support::TestVault,
    String,
    String,
) {
    let vault = test_vault();
    let anna = vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Anna", "Beck", vec![value("phone", "mobile", "0170 1")]),
        )
        .await
        .expect("create");
    let device = vault.db.device_id().to_string();
    (vault, anna.id, device)
}

fn link(contact_id: &str, raw: i64, version: i64) -> Link {
    Link {
        contact_id: contact_id.to_owned(),
        raw_contact_id: raw,
        lookup_key: "lookup".to_owned(),
        account_type: String::new(),
        account_name: String::new(),
        source_id: None,
        version,
        read_only: false,
        base: Base {
            holzi_updated_at: "t".to_owned(),
            ..Base::default()
        },
        photo_base: None,
    }
}

#[tokio::test]
async fn a_link_is_updated_in_place_and_remembered_on_this_phone() {
    let (vault, anna, device) = vault_with_anna().await;
    let (d, a) = (device.clone(), anna.clone());
    vault
        .db
        .write_blocking(move |tx| {
            save_link(tx, &d, &link(&a, 42, 1)).map_err(haex_crdt::Error::from)?;
            save_link(tx, &d, &link(&a, 42, 5)).map_err(haex_crdt::Error::from)
        })
        .expect("write");
    let d = device.clone();
    let (links, known) = vault
        .db
        .read_blocking(move |q| Ok((links(q, &d)?, known(q)?)))
        .expect("read");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].version, 5);
    assert_eq!(links[0].base.holzi_updated_at, "t");
    assert_eq!(known.get(&42), Some(&anna));
}

#[tokio::test]
async fn the_device_local_list_outlives_the_contact() {
    let (vault, anna, device) = vault_with_anna().await;
    let (d, a) = (device.clone(), anna.clone());
    vault
        .db
        .write_blocking(move |tx| save_link(tx, &d, &link(&a, 42, 1)).map_err(Into::into))
        .expect("write");
    vault
        .service
        .delete(&Caller::User, &[], vec![anna.clone()])
        .await
        .expect("delete");
    let d = device.clone();
    let (links, known) = vault
        .db
        .read_blocking(move |q| Ok((links(q, &d)?, known(q)?)))
        .expect("read");
    assert!(links.is_empty(), "the link went with the contact");
    assert_eq!(
        known.get(&42),
        Some(&anna),
        "the phone still knows what to delete"
    );
}

#[tokio::test]
async fn targets_and_issues_have_one_row_per_contact_and_phone() {
    let (vault, anna, device) = vault_with_anna().await;
    let (d, a) = (device.clone(), anna.clone());
    vault
        .db
        .write_blocking(move |tx| {
            let target = |name: &str| Target {
                contact_id: a.clone(),
                account_type: "com.google".to_owned(),
                account_name: name.to_owned(),
            };
            set_target(tx, &d, &target("first")).map_err(haex_crdt::Error::from)?;
            set_target(tx, &d, &target("second")).map_err(haex_crdt::Error::from)?;
            set_issue(tx, &d, &a, IssueReason::ReadOnly, None, "t1")
                .map_err(haex_crdt::Error::from)?;
            set_issue(
                tx,
                &d,
                &a,
                IssueReason::Failed,
                Some("OperationApplicationException"),
                "t2",
            )
            .map_err(haex_crdt::Error::from)
        })
        .expect("write");
    let d = device.clone();
    let (found_targets, found_issues) = vault
        .db
        .read_blocking(move |q| Ok((targets(q, &d)?, issues(q, &d)?)))
        .expect("read");
    assert_eq!(found_targets.len(), 1);
    assert_eq!(found_targets[0].account_name, "second");
    assert_eq!(found_issues.len(), 1);
    assert_eq!(found_issues[0].reason, IssueReason::Failed);

    let (d, a) = (device.clone(), anna.clone());
    vault
        .db
        .write_blocking(move |tx| clear_issue(tx, &d, &a).map_err(Into::into))
        .expect("clear");
    let d = device.clone();
    assert!(vault
        .db
        .read_blocking(move |q| Ok(issues(q, &d)?))
        .expect("read")
        .is_empty());
}

#[tokio::test]
async fn the_time_of_a_change_comes_from_the_hlc_of_its_columns() {
    let (vault, anna, _) = vault_with_anna().await;
    let before = crate::passwords::clock::unix_millis(std::time::SystemTime::now()) - 60_000;
    let id = anna.clone();
    let millis = vault
        .db
        .read_blocking(move |q| Ok(changed_millis(q, "haex_contacts", &id, &["given_name"])?))
        .expect("read")
        .expect("a time");
    assert!(millis > before, "{millis} is the time of the create");
    assert_eq!(hlc_millis("not an hlc"), None);
}
