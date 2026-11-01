# Built-in voices

Reference recordings Pocket TTS imitates (spec 066, research R8). Two to three per language, native
speakers only, and only from sources that allow commercial use: CC0 or CC BY. Expresso, EARS and
other non-commercial sources are excluded.

Every file is mono, 16-bit PCM at 24 kHz, trimmed to at most 15 s, with peaks at most -1 dBFS.
Names of anonymous dataset speakers are invented; CC BY credit is in `voices.json` (`attribution`)
and shown with the app's licences.

| Id           | Name    | Language | Default | Source                                                                            | Licence   | Origin                                                                                                 |
| ------------ | ------- | -------- | ------- | --------------------------------------------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------ |
| `de-lena`    | Lena    | de       | yes     | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts german/dev, speaker 8325, 8325_6882_000083.wav                                        |
| `de-jonas`   | Jonas   | de       |         | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts german/dev, speaker 10870, 10870_10823_000099-0001.wav                                |
| `en-caro`    | Caro    | en       | yes     | [Voice-Zero (LibriVox recordings)](https://github.com/OwenTyme/voice-zero)        | CC0 1.0   | kyutai/tts-voices@323332d33f997de8394f24a193e1a76df720e01a voice-zero/caro_davy.wav                    |
| `en-bill`    | Bill    | en       |         | [Voice-Zero (LibriVox recordings)](https://github.com/OwenTyme/voice-zero)        | CC0 1.0   | kyutai/tts-voices@323332d33f997de8394f24a193e1a76df720e01a voice-zero/bill_boerst.wav                  |
| `fr-camille` | Camille | fr       | yes     | [Kyutai Unmute voices (own recordings)](https://huggingface.co/kyutai/tts-voices) | CC0 1.0   | kyutai/tts-voices@323332d33f997de8394f24a193e1a76df720e01a unmute-prod-website/developpeuse-3.wav      |
| `fr-louis`   | Louis   | fr       |         | [Kyutai Unmute voices (own recordings)](https://huggingface.co/kyutai/tts-voices) | CC0 1.0   | kyutai/tts-voices@323332d33f997de8394f24a193e1a76df720e01a unmute-prod-website/fabieng-enhanced-v2.wav |
| `es-lucia`   | Lucía   | es       | yes     | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts spanish/dev, speaker 3473, 3473_1378_000096.wav                                       |
| `es-diego`   | Diego   | es       |         | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts spanish/dev, speaker 4589, 4589_1378_000007-0001.wav                                  |
| `it-giulia`  | Giulia  | it       | yes     | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts italian/dev, speaker 11247, 11247_8906_000026-0002.wav                                |
| `it-marco`   | Marco   | it       |         | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts italian/dev, speaker 645, 645_529_000051-0001.wav                                     |
| `pt-ana`     | Ana     | pt       | yes     | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts portuguese/dev, speaker 9056, 9056_8708_000017-0001.wav                               |
| `pt-pedro`   | Pedro   | pt       |         | [CML-TTS (LibriVox audiobook recordings)](https://www.openslr.org/146/)           | CC BY 4.0 | ylacombe/cml-tts portuguese/dev, speaker 5417, 5417_3702_000077.wav                                    |

## Sources

- **CML-TTS** (Oliveira et al. 2023, <https://www.openslr.org/146/>, mirrored as
  `ylacombe/cml-tts` on Hugging Face): LibriVox audiobook recordings, CC BY 4.0. The dataset asks
  users not to identify its speakers; only the speaker number is given.
- **Voice-Zero** (<https://github.com/OwenTyme/voice-zero>): cleaned LibriVox recordings, CC0;
  taken from the copy in `kyutai/tts-voices`.
- **Kyutai Unmute** (`kyutai/tts-voices`, folder `unmute-prod-website`): Kyutai's own recordings,
  CC0 according to the repository's README.

## Adding or changing a voice

1. Put the WAV here as `<language>-<name>.wav` (mono, 16-bit PCM, 5–20 s).
2. Add its entry to `voices.json` and its file name to `AUDIO` in `../voices.rs`.
3. Add a row above.
4. Run `cargo test tts::voices` and listen with the ignored test `builtin_voices_speak`.
