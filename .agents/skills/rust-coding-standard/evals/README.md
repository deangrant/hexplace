# rust-coding-standard evals

Run these cases to test whether `rust-coding-standard` improves agent
output. They follow
[Evaluating skill output quality](https://agentskills.io/skill-creation/evaluating-skills).

- `evals.json`: cases with a prompt, expected output, optional input files,
  and assertions.
- `files/`: input fixtures. Each fixture compiles. Each fixture contains known
  violations, and each violation maps to one assertion.

## Fixtures

| File | Case | Contents |
| --- | --- | --- |
| `undocumented.rs` | 2 | Public items with no rustdoc; one private helper; a slice index |
| `concrete_dependency.rs` | 3 | `process_chunk` constructs `ObjectStoreSink` and calls only `put` |
| `review_me.rs` | 5 | Nine style and lint violations for a review |
| `held_lock.rs` | 10 | Mutex guard held across `.await`; `Rc<RefCell<_>>` on a public type |
| `legacy_parser.rs` | 11 | `&Vec`, `get_` getter, `+` concatenation; empty input returns `"unknown"` |

## Run one case

1. Start a clean session for each run.
2. Give the agent the case `prompt` and any `files` listed for that case.
3. For a baseline, omit the skill. To compare versions, point the run at a
   snapshot of the older skill.
4. Save the output under a workspace directory beside the skill, for example
   `rust-coding-standard-workspace/iteration-1/eval-1/with_skill/outputs/`.
5. Grade the assertions with an LLM or by hand. A PASS needs quoted evidence
   from the output. To grade code mechanically, compile the output in a
   scratch crate and run `cargo clippy -- -D warnings` with the workspace
   lints.

## Baseline protocol (`without_skill`)

`without_skill` runs must not read repository policy. Block these paths for
the baseline agent:

- `AGENTS.md`
- root `Cargo.toml`
- `clippy.toml`
- `.agents/skills/**`

Cases 5, 6, and 8 also tell the agent not to consult `AGENTS.md` or
`Cargo.toml` for policy. That line applies to both arms. The `with_skill`
arm must carry lint and unsafe policy from the skill alone.

## Grader note for case 5

A reviewer who flags an issue that is not planted, for example missing docs,
still passes the case.
