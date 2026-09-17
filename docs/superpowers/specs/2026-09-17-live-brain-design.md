# Live company brain

Approved direction: evolve the Rust/SQLite/React application rather than replace it. Use the customer's installed CLI and existing durable queue. Keep all model output proposed until the owner approves evidence-backed knowledge. No automatic provider switching or extra paid API.

## Build runtime

Replace the supervisor's single truncated corpus with bounded, lossless text batches. Persist successful batch replies under a content-derived key (including company context, engine and prompt version), validate cached replies before reuse, and checkpoint each batch. A retry processes unfinished batches. Keep request history bounded and deliver the actual corpus on the first request. Preserve approved objects when a new synthesis proposes the same identifier. Invalid evidence must never enter the review quiz.

The worker keeps its job lease alive during supervision. Temporary CLI failures remain retryable; unavailable credentials or malformed output surface as actionable failures. Child output and process lifetime must be bounded. Existing provider adapters remain the integration boundary.

## Live projection

Keep `/api/brain` approved-only. Add `/api/brain/build` as an authenticated owner-only projection of extracted documents, proposed/conflicted/approved objects, and evidence links. Return real counts; do not launch assistant detection on this frequent path. This derived view does not change MCP visibility or imply approval.

## Interface

Use the same 3D brain for build and approved views. Preserve node positions, render updates only when graph content changes, correctly retain kind-specific node radii, reduce detail for large graphs, honor reduced motion and stop hidden rendering. A searchable list supports keyboard navigation and environments without WebGL.

Replace the staged illustration and inferred progress percentage with the actual build projection, queue status, evidence relationships, document and discovery counts. Show failures, pauses and disconnected states explicitly. Do not advance on failed or empty builds. Keep review an explicit next action. Prevent duplicate starts and surface startup errors. Persist company context before queueing work.

## Verification

Regression tests cover first-call context, bounded/lossless batching, checkpoint reuse, evidence/approval boundaries and the build projection. Run relevant Rust tests and workspace checks, frontend build/lint and browser checks of building, paused, failed, completed and responsive states. Use replay/fixtures for validation; do not consume the owner's CLI allowance for tests. Commit each verified implementation stage separately.
