use super::*;
use crate::contacts::service::test_support::{person, test_vault};
use crate::error::HolziError;

#[tokio::test]
async fn groups_are_created_renamed_and_filled() {
    let vault = test_vault();
    let group = vault
        .service
        .group_save(&Caller::User, None, " Arbeit ".into())
        .await
        .expect("create");
    assert_eq!(group.name, "Arbeit");
    let renamed = vault
        .service
        .group_save(&Caller::User, Some(group.id.clone()), "Büro".into())
        .await
        .expect("rename");
    assert_eq!(renamed.id, group.id);
    assert_eq!(
        vault.service.groups(&Caller::User).await.expect("groups"),
        std::slice::from_ref(&renamed)
    );

    let anna = vault
        .service
        .create(&Caller::User, &[], person("Anna", "Beck", vec![]))
        .await
        .expect("create");
    for _ in 0..2 {
        vault
            .service
            .group_members(
                &Caller::User,
                group.id.clone(),
                vec![anna.id.clone()],
                vec![],
            )
            .await
            .expect("add, twice changes nothing");
    }
    let member = vault
        .service
        .get(&Caller::User, &[], &anna.id)
        .await
        .expect("get");
    assert_eq!(member.groups, [renamed]);

    vault
        .service
        .group_members(
            &Caller::User,
            group.id.clone(),
            vec![],
            vec![anna.id.clone()],
        )
        .await
        .expect("remove");
    let gone = vault
        .service
        .get(&Caller::User, &[], &anna.id)
        .await
        .expect("get");
    assert!(gone.groups.is_empty());
}

#[tokio::test]
async fn deleting_a_group_keeps_its_contacts() {
    let vault = test_vault();
    let group = vault
        .service
        .group_save(&Caller::User, None, "Familie".into())
        .await
        .expect("create");
    let mut input = person("Ben", "Krüger", vec![]);
    input.groups = vec![group.id.clone()];
    let ben = vault
        .service
        .create(&Caller::User, &[], input)
        .await
        .expect("create");
    vault
        .service
        .group_delete(&Caller::User, group.id)
        .await
        .expect("delete");
    let still = vault
        .service
        .get(&Caller::User, &[], &ben.id)
        .await
        .expect("still there");
    assert!(still.groups.is_empty());
}

#[tokio::test]
async fn groups_are_for_the_user_alone_and_need_a_name() {
    let vault = test_vault();
    assert!(matches!(
        vault.service.groups(&Caller::BuiltinAgent).await,
        Err(HolziError::ContactsForbidden)
    ));
    assert!(matches!(
        vault
            .service
            .group_save(&Caller::User, None, "  ".into())
            .await,
        Err(HolziError::ContactsInvalid { .. })
    ));
    assert!(matches!(
        vault
            .service
            .group_save(&Caller::User, Some("missing".into()), "X".into())
            .await,
        Err(HolziError::ContactsNotFound)
    ));
}

#[tokio::test]
async fn a_contact_cannot_join_a_group_that_does_not_exist() {
    let vault = test_vault();
    let mut input = person("Ben", "Krüger", vec![]);
    input.groups = vec!["missing".into()];
    assert!(matches!(
        vault.service.create(&Caller::User, &[], input).await,
        Err(HolziError::ContactsInvalid { .. })
    ));
}
