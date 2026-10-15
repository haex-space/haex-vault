//! Photos from holzi to the phone in whole runs against the fake phone (spec 051 T054).

use crate::contacts::model::{ContactInput, Head, TargetInput};
use crate::contacts::phone::test_rig::Rig;
use crate::contacts::phone::test_support::{name, GOOGLE};
use crate::contacts::phone::Mode;
use crate::contacts::service::Caller;

fn png(shade: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        32,
        32,
        image::Rgb([shade, 0, 0]),
    ))
    .write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .expect("a png");
    bytes
}

#[tokio::test]
async fn a_photo_set_in_holzi_goes_to_the_phone_once() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    rig.start();
    let anna = rig.named("Anna Beck").id;
    let set = rig
        .vault
        .service
        .set_photo(&Caller::User, &anna, png(200))
        .await
        .expect("photo");
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.book.photo_writes(), [raw]);
    assert_eq!(rig.links()[0].photo_base, set.photo);

    rig.run(Mode::Normal).expect("again");
    assert_eq!(rig.book.photo_writes(), [raw], "written once");
    assert!(
        rig.book.ops().is_empty(),
        "the photo's new version is no change"
    );
}

#[tokio::test]
async fn a_photo_taken_over_from_the_phone_is_not_written_back() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    rig.book.set_photo(raw, png(10));
    rig.start();
    let anna = rig.named("Anna Beck").id;
    // Any change of the contact runs it through the comparison again.
    rig.vault
        .service
        .group_save(&Caller::User, None, "Arbeit".into())
        .await
        .expect("group");
    let _ = anna;
    rig.run(Mode::Normal).expect("run");
    assert!(rig.book.photo_writes().is_empty());
}

#[tokio::test]
async fn a_new_contact_goes_to_the_phone_with_its_photo() {
    let rig = Rig::new();
    rig.start();
    let ben = rig
        .vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                targets: vec![TargetInput {
                    device: rig.device.clone(),
                    account: None,
                }],
                ..ContactInput::default()
            },
        )
        .await;
    // Without a target account the phone has nothing to suggest yet; set one and try again.
    assert!(ben.is_err());
    let device = rig.device.clone();
    rig.vault
        .db
        .write_blocking(move |tx| {
            let google = crate::contacts::phone::Account {
                account_type: GOOGLE.0.to_owned(),
                name: GOOGLE.1.to_owned(),
                writable: true,
                is_default: false,
            };
            crate::contacts::phone::state::set_target(tx, &device, &google)
                .map_err(haex_crdt::Error::from)
        })
        .expect("target account");
    let ben = rig
        .vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                targets: vec![TargetInput {
                    device: rig.device.clone(),
                    account: None,
                }],
                ..ContactInput::default()
            },
        )
        .await
        .expect("create")
        .id;
    rig.vault
        .service
        .set_photo(&Caller::User, &ben, png(90))
        .await
        .expect("photo");
    rig.run(Mode::Normal).expect("run");
    let live = rig.book.live();
    assert_eq!(live.len(), 1);
    assert_eq!(rig.book.photo_writes(), [live[0].index.id]);
    assert!(rig.links()[0].photo_base.is_some());
}
