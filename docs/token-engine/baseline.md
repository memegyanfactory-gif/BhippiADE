# Bhippi Token Engine — baseline

Captured on the offline demo provider; deterministic and rerunnable.

## Per-task context budget

| task | effort | design | history msgs | input est. | reserved output | stream reqs | handoff |
|---|---|---|---|---|---|---|---|
| short_question | fast | off | 1 | 1064 | 512 | 1 | no |
| research_deep_dive | quality | off | 1 | 4681 | 4096 | 1 | no |
| code_task | balanced | off | 1 | 2646 | 2048 | 1 | no |
| follow_up_one | balanced | off | 3 | 2921 | 2048 | 1 | no |
| design_brief | ultra | on | 1 | 9950 | 8192 | 1 | no |

## Category breakdown

| task | category | estimated tokens |
|---|---|---|
| short_question | reserved_response | 512 |
| short_question | workspace | 272 |
| short_question | project_rules | 199 |
| short_question | system | 46 |
| short_question | conversation | 18 |
| short_question | task_directives | 14 |
| short_question | skills | 0 |
| short_question | computer_use | 0 |
| short_question | engine | 0 |
| short_question | handoff | 0 |
| research_deep_dive | reserved_response | 4096 |
| research_deep_dive | workspace | 272 |
| research_deep_dive | project_rules | 199 |
| research_deep_dive | system | 46 |
| research_deep_dive | conversation | 46 |
| research_deep_dive | task_directives | 19 |
| research_deep_dive | skills | 0 |
| research_deep_dive | computer_use | 0 |
| research_deep_dive | engine | 0 |
| research_deep_dive | handoff | 0 |
| code_task | reserved_response | 2048 |
| code_task | workspace | 272 |
| code_task | project_rules | 199 |
| code_task | conversation | 64 |
| code_task | system | 46 |
| code_task | task_directives | 14 |
| code_task | skills | 0 |
| code_task | computer_use | 0 |
| code_task | engine | 0 |
| code_task | handoff | 0 |
| follow_up_one | reserved_response | 2048 |
| follow_up_one | conversation | 339 |
| follow_up_one | workspace | 272 |
| follow_up_one | project_rules | 199 |
| follow_up_one | system | 46 |
| follow_up_one | task_directives | 14 |
| follow_up_one | skills | 0 |
| follow_up_one | computer_use | 0 |
| follow_up_one | engine | 0 |
| follow_up_one | handoff | 0 |
| design_brief | reserved_response | 8192 |
| design_brief | task_directives | 1200 |
| design_brief | workspace | 272 |
| design_brief | project_rules | 199 |
| design_brief | system | 46 |
| design_brief | conversation | 38 |
| design_brief | skills | 0 |
| design_brief | computer_use | 0 |
| design_brief | engine | 0 |
| design_brief | handoff | 0 |

## Summary

- Mean estimated input per task: **4252 tokens**
- Tool-schema overhead (A3): **0 tokens** — no tool schemas are injected into requests today, which is the measured fact.
- Multi-provider handoff overhead (A4): **0 observed turn(s)**; injecting the note adds **81 estimated tokens** when it fires.
- Repository-context overhead (A5): see the workspace/project_rules/engine rows above — a summed mean folds in when Phase B compares against this baseline.

Sample log: `baseline.json`.

## Engine dynamic-context boundary (ENG-191)

Measured with the same four-bytes-per-token estimator used by `ContextSampleStore`. The
fixture is deterministic (`Crate_N: cube at [N, 0, 0]`) and the 1,000-entity case proves
retrieval rather than injection: the tail entity is absent and the context ends by directing
the model to `engine_query`.

| scene fixture | uncapped estimate | emitted estimate | result |
|---|---:|---:|---|
| empty | 7 | 7 | complete map |
| 50 entities | 390 | 390 | complete map |
| 1,000 entities | 8,203 | ≤1,520 | capped at 1,500 + retrieval suffix |

Guard: `chat::tests::engine_context_budget_is_scene_size_independent`.
