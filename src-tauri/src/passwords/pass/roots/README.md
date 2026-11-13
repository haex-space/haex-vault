# Apple certificates for the pkpass signature check

Public certificates (DER), embedded with `include_bytes!` in `../signature.rs` for the optional
signature check of a `.pkpass` file (spec 092, FR-030, task T067). They are public data published by
Apple; nothing secret is in this directory. There is no `bundle.resources` in the app, so a resource
directory would not ship on Android; that is why they are compiled in.

Downloaded on 2026-11-13 from <https://www.apple.com/certificateauthority/> (Apple Certificate
Authority page). The SHA-256 fingerprints below are of the files as downloaded.

## Trust anchors (roots)

| File                          | Source                                                          | Subject            | Valid until | SHA-256                                                            |
| ----------------------------- | --------------------------------------------------------------- | ------------------ | ----------- | ------------------------------------------------------------------ |
| `AppleIncRootCertificate.cer` | <https://www.apple.com/appleca/AppleIncRootCertificate.cer>     | Apple Root CA      | 2035-02-09  | `B0B1730ECBC7FF4505142C49F1295E6EDA6BCAED7E2C68C5BE91B5A11001F024` |
| `AppleRootCA-G2.cer`          | <https://www.apple.com/certificateauthority/AppleRootCA-G2.cer> | Apple Root CA - G2 | 2039-04-30  | `C2B9B042DD57830E7D117DAC55AC8AE19407D38E41D88F3215BC3A890444A050` |
| `AppleRootCA-G3.cer`          | <https://www.apple.com/certificateauthority/AppleRootCA-G3.cer> | Apple Root CA - G3 | 2039-04-30  | `63343ABFB89A6A03EBB57E9B3F5FA7BE7C4F5C756F3017B3A8C488C3653E9179` |

## Intermediates (helpers, never trusted without a chain to a root)

Apple Worldwide Developer Relations certification authorities, the issuers of the pass type ID
certificates a pass is signed with. A pass normally carries its intermediate in its signature; these
are for one that does not.

| File                | Source                                                                                                          | Issuer             | Valid until | SHA-256                                                            |
| ------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------ | ----------- | ------------------------------------------------------------------ |
| `AppleWWDRCAG1.cer` | <https://developer.apple.com/certificationauthority/AppleWWDRCA.cer> (G1 is no longer listed on the Apple page) | Apple Root CA      | 2023-02-07  | `CE057691D730F89CA25E916F7335F4C8A15713DCD273A658C024023F8EB809C2` |
| `AppleWWDRCAG2.cer` | <https://www.apple.com/certificateauthority/AppleWWDRCAG2.cer>                                                  | Apple Root CA - G3 | 2029-05-06  | `9ED4B3B88C6A339CF1387895BDA9CA6EA31A6B5CE9EDF7511845923B0C8AC94C` |
| `AppleWWDRCAG3.cer` | <https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer>                                                  | Apple Root CA      | 2030-02-20  | `DCF21878C77F4198E4B4614F03D696D89C66C66008D4244E1B99161AAC91601F` |
| `AppleWWDRCAG4.cer` | <https://www.apple.com/certificateauthority/AppleWWDRCAG4.cer>                                                  | Apple Root CA      | 2030-12-10  | `EA4757885538DD8CB59FF4556F676087D83C85E70902C122E42C0808B5BCE14C` |
| `AppleWWDRCAG5.cer` | <https://www.apple.com/certificateauthority/AppleWWDRCAG5.cer>                                                  | Apple Root CA      | 2030-12-10  | `53FD008278E5A595FE1E908AE9C5E5675F26243264A5A6438C023E3CE2870760` |
| `AppleWWDRCAG6.cer` | <https://www.apple.com/certificateauthority/AppleWWDRCAG6.cer>                                                  | Apple Root CA - G3 | 2036-03-19  | `BDD4ED6E74691F0C2BFD01BE0296197AF1379E0418E2D300EFA9C3BEF642CA30` |

The check validates each certificate at the time the pass was signed, not today, so an expired
intermediate such as G1 still verifies passes that were signed while it was valid.

## Updating

When Apple publishes a new root or intermediate, add the DER file here, list it in `../signature.rs`
and in this table. The test `signature_tests::embedded_certificates_all_parse_and_chain` fails when a
file is not a DER certificate or does not chain to a root listed there.
