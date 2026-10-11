use super::*;
use crate::files::encrypted::format::vectors::{check_contents, parse, V1};

const DEK: [u8; 32] = [4u8; 32];
const CID: Id = [8u8; 16];

/// xorshift64*, seeded per test so a failure repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

fn plaintext(rng: &mut Rng, size: usize) -> Vec<u8> {
    (0..size).map(|_| rng.next() as u8).collect()
}

/// Sealed in pieces of random length, as a transfer delivers them.
fn sealed_in_pieces(rng: &mut Rng, plain: &[u8]) -> Vec<u8> {
    let mut sealer = ContentSealer::with_parts(&DEK, &CID, &[2u8; 16]);
    let mut object = Vec::new();
    let mut rest = plain;
    while !rest.is_empty() {
        let take = (rng.below(3 * CHUNK as u64 / 2) as usize + 1).min(rest.len());
        sealer.push(&rest[..take], &mut object);
        rest = &rest[take..];
    }
    sealer.finish(&mut object);
    object
}

fn read_all(object: &[u8], size: u64) -> Result<Vec<u8>, FormatError> {
    let opener = ContentOpener::new(object, &CID, &DEK, size)?;
    opener.check_len(object.len() as u64)?;
    let mut plain = Vec::new();
    for index in 0..opener.block_count() {
        let span = opener.block_span(index)?;
        plain.extend(opener.open_block(index, &object[span.start as usize..span.end as usize])?);
    }
    Ok(plain)
}

/// Sizes around every block boundary up to three blocks, plus random ones.
fn sizes(rng: &mut Rng) -> Vec<usize> {
    let mut sizes = vec![0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, 2 * CHUNK + 1];
    sizes.extend((0..8).map(|_| rng.below(3 * CHUNK as u64) as usize));
    sizes
}

#[test]
fn the_contents_match_the_vectors_and_every_content_case_is_refused() {
    let vectors = parse(V1).unwrap();
    assert!(check_contents(&vectors).unwrap() > 0);
}

#[test]
fn the_length_follows_the_size() {
    assert_eq!(ciphertext_len(0), 38 + 16);
    assert_eq!(ciphertext_len(1), 38 + 16 + 1);
    assert_eq!(ciphertext_len(65_536), 38 + 16 + 65_536);
    assert_eq!(ciphertext_len(65_537), 38 + 32 + 65_537);
    assert_eq!(block_count(0), 1);
    assert_eq!(block_count(2 * 65_536), 2);
}

#[test]
fn random_sizes_round_trip_in_random_pieces() {
    let mut rng = Rng(0x0048_0001);
    for size in sizes(&mut rng) {
        let plain = plaintext(&mut rng, size);
        let object = sealed_in_pieces(&mut rng, &plain);
        assert_eq!(object.len() as u64, ciphertext_len(size as u64), "{size}");
        assert_eq!(read_all(&object, size as u64).unwrap(), plain, "{size}");
    }
}

#[test]
fn a_multiple_of_chunk_ends_with_a_full_last_block() {
    let mut rng = Rng(7);
    let plain = plaintext(&mut rng, 2 * CHUNK);
    let object = sealed_in_pieces(&mut rng, &plain);
    let opener = ContentOpener::new(&object, &CID, &DEK, plain.len() as u64).unwrap();
    assert_eq!(opener.block_count(), 2);
    let last = opener.block_span(1).unwrap();
    assert_eq!(last.end, object.len() as u64);
    assert_eq!(last.end - last.start, (CHUNK + TAG_LEN) as u64);
}

#[test]
fn random_ranges_open_to_the_same_bytes() {
    let mut rng = Rng(0x0048_0002);
    let plain = plaintext(&mut rng, 2 * CHUNK + 777);
    let object = sealed_in_pieces(&mut rng, &plain);
    let opener = ContentOpener::new(&object, &CID, &DEK, plain.len() as u64).unwrap();
    for _ in 0..40 {
        let a = rng.below(plain.len() as u64);
        let b = a + 1 + rng.below(plain.len() as u64 - a);
        let range = opener.ciphertext_range(a, b).unwrap();
        let part = opener
            .open_range(a, b, &object[range.start as usize..range.end as usize])
            .unwrap();
        assert_eq!(part, plain[a as usize..b as usize], "[{a}, {b})");
    }
}

#[test]
fn a_range_outside_the_file_is_refused() {
    let object = sealed_in_pieces(&mut Rng(1), &[1, 2, 3]);
    let opener = ContentOpener::new(&object, &CID, &DEK, 3).unwrap();
    for (a, b) in [(0, 0), (2, 1), (0, 4), (3, 4)] {
        assert!(matches!(
            opener.ciphertext_range(a, b),
            Err(FormatError::OutOfRange)
        ));
    }
    assert!(matches!(opener.block_span(1), Err(FormatError::OutOfRange)));
}

#[test]
fn any_truncation_or_extension_is_refused() {
    let mut rng = Rng(0x0048_0003);
    for size in sizes(&mut rng) {
        let plain = plaintext(&mut rng, size);
        let object = sealed_in_pieces(&mut rng, &plain);
        for _ in 0..6 {
            let cut = rng.below(object.len() as u64) as usize;
            // Also as if the sidecar's size matched the cut object.
            let claimed = (cut.saturating_sub(HEADER_LEN + TAG_LEN)) as u64;
            for size in [size as u64, claimed] {
                assert!(read_all(&object[..cut], size).is_err(), "{size} cut {cut}");
            }
            let mut longer = object.clone();
            let extra = 1 + rng.below(40) as usize;
            longer.extend(plaintext(&mut rng, extra));
            assert!(read_all(&longer, size as u64).is_err(), "{size} extended");
        }
    }
}

#[test]
fn a_cut_at_a_block_boundary_is_refused_even_with_a_matching_size() {
    let mut rng = Rng(11);
    let plain = plaintext(&mut rng, 2 * CHUNK + 5);
    let object = sealed_in_pieces(&mut rng, &plain);
    for blocks in [1, 2] {
        let cut = HEADER_LEN + blocks * (CHUNK + TAG_LEN);
        let size = (blocks * CHUNK) as u64;
        assert_eq!(ciphertext_len(size), cut as u64);
        assert!(matches!(
            read_all(&object[..cut], size),
            Err(FormatError::Decrypt)
        ));
    }
}

#[test]
fn swapped_blocks_are_refused() {
    let mut rng = Rng(0x0048_0004);
    let plain = plaintext(&mut rng, 3 * CHUNK);
    let object = sealed_in_pieces(&mut rng, &plain);
    let block = CHUNK + TAG_LEN;
    for (a, b) in [(0, 1), (1, 2), (0, 2)] {
        let mut swapped = object.clone();
        let (head, tail) = swapped.split_at_mut(HEADER_LEN + b * block);
        head[HEADER_LEN + a * block..HEADER_LEN + (a + 1) * block]
            .swap_with_slice(&mut tail[..block]);
        assert!(
            matches!(
                read_all(&swapped, plain.len() as u64),
                Err(FormatError::Decrypt)
            ),
            "{a} <-> {b}"
        );
    }
}

#[test]
fn another_cid_or_key_is_refused() {
    let object = sealed_in_pieces(&mut Rng(3), b"inhalt");
    assert!(matches!(
        ContentOpener::new(&object, &[0u8; 16], &DEK, 6),
        Err(FormatError::Mismatch)
    ));
    let opener = ContentOpener::new(&object, &CID, &[0u8; 32], 6).unwrap();
    let span = opener.block_span(0).unwrap();
    assert!(matches!(
        opener.open_block(0, &object[span.start as usize..]),
        Err(FormatError::Decrypt)
    ));
}

#[test]
fn a_fresh_sealer_draws_a_new_key_and_cid() {
    let a = ContentSealer::fresh();
    let b = ContentSealer::fresh();
    assert_ne!(a.dek(), b.dek());
    assert_ne!(a.cid(), b.cid());
    let cid = a.cid();
    let mut object = Vec::new();
    a.finish(&mut object);
    assert_eq!(object[6..22], cid);
    assert_eq!(object.len() as u64, ciphertext_len(0));
}
