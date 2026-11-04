# Results: A harness that lets small local models do everyday tasks

Every model before and after each delivery step (plan, Delivery order), measured with the everyday set of spec 078
as extended by this spec (contracts/everyday-report.md).

## Models

The local files of research R10 (Q4_K_M, the reference laptop's models folder), checked 2026-11-04:

| Model        | File                                    | Size    |
| ------------ | --------------------------------------- | ------- |
| Qwen3.5 2B   | `Qwen_Qwen3.5-2B-Q4_K_M.gguf`           | 1.40 GB |
| Qwen3.5 4B   | `Qwen_Qwen3.5-4B-Q4_K_M.gguf`           | 3.01 GB |
| Qwen3.5 9B   | `Qwen_Qwen3.5-9B-Q4_K_M.gguf`           | 6.17 GB |
| Qwen3 1.7B   | `Qwen_Qwen3-1.7B-Q4_K_M.gguf`           | 1.28 GB |
| Gemma 4 E2B  | `gemma-4-E2B-it-Q4_K_M.gguf`            | 3.11 GB |
| Gemma 4 E4B  | `gemma-4-E4B-it-Q4_K_M.gguf`            | 4.98 GB |
| Gemma 4 12B  | `gemma-4-12b-it-Q4_K_M.gguf`            | 7.12 GB |
| Hermes 4 14B | `NousResearch_Hermes-4-14B-Q4_K_M.gguf` | 9.00 GB |

Hosted models run through the subscription with the `claude` CLI (spec 078 quickstart), as through their API
(research R2).

## Baseline

_Pending: T018 and T019._
