/**
 * Domain types.
 *
 * These mirror what the Rust daemon serves over its local HTTP API. The UI
 * never invents a field the compiler cannot produce: every displayed claim is
 * reachable from `Evidence` with a byte range into a real source document.
 */

export type ObjectKind = "rule" | "process" | "term" | "skill" | "fact"

export type ObjectStatus =
  /** Compiled, waiting for a human. Never served to agents. */
  | "draft"
  /** A person approved it. This is the only status agents can read. */
  | "approved"
  /** Replaced by a newer object; kept for history. */
  | "superseded"
  /** Two approved-or-candidate objects disagree. Blocks approval. */
  | "conflict"
  | "rejected"

/**
 * A quiet second axis under Knowledge. It changes the label on an object, never
 * the navigation — the owner should not have to learn a taxonomy to find things.
 */
export type KnowledgeType = "term" | "product" | "policy" | "reference" | "template"

export type SourceKind = "folder" | "nas"
export type Processor = "codex" | "claude-code" | "cursor-agent" | "managed"

/** A literal span in a source document. The unit the evidence gate checks. */
export interface Evidence {
  id: string
  documentId: string
  /** File name shown to a person, e.g. "Popusti-2026.docx". */
  documentName: string
  /** Human locator: "paragraph 4", "page 12", "sheet Cjenik · row 18". */
  locator: string
  startByte: number
  endByte: number
  /** The exact text at [startByte, endByte). Never paraphrased. */
  quote: string
  /** Set when the document has pages (PDF). */
  page?: number
  /** Mechanical gate: does the quote still exist at that offset? */
  verified: boolean
}

export type RelationType =
  | "depends_on"
  | "derived_from"
  | "supersedes"
  | "conflicts_with"
  | "used_by"

/**
 * Where an edge came from.
 *
 * `structural` is a string match and is right when it fires. `model` is the
 * engine's proposal: dependency between two rules is rarely written down
 * anywhere, so on a real folder structural detection found one edge across
 * fifty-one objects, and a graph with one edge answers "what breaks if I
 * change this" with silence. A proposed edge has no sentence behind it and
 * therefore nothing for the evidence gate to check, so it is shown as
 * suggested and can be removed rather than being presented as fact.
 */
export type RelationOrigin = "structural" | "model" | "manual"

export interface Relation {
  type: RelationType
  targetId: string
  /** Denormalised for display; the daemon sends it alongside the edge. */
  targetTitle: string
  origin?: RelationOrigin
}

export interface ContextObject {
  id: string
  kind: ObjectKind
  /** Only meaningful for `term` and `fact`. */
  subtype?: KnowledgeType
  title: string
  /** Canonical Markdown. The source of truth, not a rendering of a graph. */
  body: string
  path: string
  evidence: Evidence[]
  relations: Relation[]
  /** 0..1, calibrated from source agreement — not a model's self-report. */
  confidence: number
  status: ObjectStatus
  version: number
  validFrom: string
  validTo: string | null
  supersedes: string | null
  updatedAt: string
  /** Set once a person decided. */
  decidedBy?: string
  /** True when the approver changed the compiler's text before approving. */
  editedOnApproval?: boolean
}

/**
 * Proof that an agent actually read an object, recorded by the gateway.
 * Never a subscription or a guess — only a read that happened.
 */
export interface ToolRead {
  tool: string
  lastReadAt: string
}

/** A compiled object waiting for review, with the diff against what exists. */
export interface ReviewItem {
  id: string
  objectId: string
  kind: ObjectKind
  title: string
  /** Null for a brand-new object. */
  before: string | null
  after: string
  evidence: Evidence[]
  confidence: number
  /** Objects that change meaning if this is approved. */
  affects: Relation[]
  conflict?: ConflictDetail
  coverage?: CoverageDetail
  compiledAt: string
}

/**
 * Two documents cover one subject, and neither states a figure.
 *
 * This is deliberately not a conflict. Nothing here can establish that two
 * differently worded paragraphs disagree, and asserting a disagreement that
 * cannot be shown is the one thing this product must not do. So it says only
 * what is checkable: both documents speak to this, here is each one's
 * sentence, and the most recent wording is the one in use.
 */
export interface CoverageDetail {
  summary: string
  sources: {
    label: string
    modified: string
    /** The wording the object uses came from this document. */
    current: boolean
    evidence: Evidence
  }[]
}

/**
 * Two objects that may be one rule written twice.
 *
 * The daemon never merges on its own: a similarity score deciding that two of
 * the company's rules are one rule deletes a rule quietly, and quiet deletion
 * is the failure this product can least afford. It asks instead, once, and
 * keeps the answer.
 */
export interface MergeHint {
  keepId: string
  keepTitle: string
  keepBody: string
  dropId: string
  dropTitle: string
  dropBody: string
  /**
   * `duplicate` — one decision written twice; merging loses nothing.
   * `disagreement` — one subject, two live figures. Held to a stricter bar,
   * because telling an owner their documents contradict each other when they
   * do not is the one claim this product must never make.
   */
  kind: "duplicate" | "disagreement"
  /** 0..1 subject overlap. Shown so the owner can see how close a call it is. */
  score: number
}

export interface ConflictDetail {
  /** Short statement of the disagreement, in the owner's vocabulary. */
  summary: string
  sides: {
    label: string
    value: string
    evidence: Evidence
  }[]
}

export interface Source {
  id: string
  name: string
  path: string
  kind: SourceKind
  access: "read-only" | "read-write"
  fileCount: number
  bytes: number
  /** Absent until the first walk has finished. */
  lastSync: string | null
  processor: Processor
  status: "active" | "paused" | "scanning" | "error"
  /** When the compiler last ran over this source, which is later than lastSync. */
  lastAnalyzed: string | null
  /** Outcomes of that run — what an owner actually wants to know. */
  changesFound: number
  conflictsFound: number
  /** File types found during the scan, largest group first. */
  fileTypes: { ext: string; count: number }[]
  /** Last folder walk — how many files changed vs stayed the same. */
  lastDigest?: {
    at: string
    walked: number
    changed: number
    unchanged: number
    unreadable: number
  }
  error?: string
}

/** One block of an original document, as the daemon extracted it. */
export interface SourceBlock {
  locator: string
  heading?: boolean
  text?: string
  cells?: string[]
}

export interface SourceDocument {
  name: string
  kind: "docx" | "pdf" | "xlsx"
  path: string
  modified: string
  /** Set for spreadsheets; `blocks` then carry `cells`. */
  columns?: string[]
  blocks: SourceBlock[]
}

export interface DiscoverySummary {
  rules: number
  processes: number
  terms: number
  skills: number
  conflicts: number
  filesRead: number
  spansExtracted: number
  durationSeconds: number
}

export interface SkillDoc {
  id: string
  name: string
  description: string
  /** The SKILL.md body a person actually reads. */
  markdown: string
  requires: Relation[]
  inputs: { name: string; type: string; description: string }[]
  outputs: { name: string; type: string; description: string }[]
  affects: Relation[]
  evidence: Evidence[]
  status: ObjectStatus
  /** Who approved it. Absent when nobody has — never a stand-in name. */
  decidedBy: string | null
  /** The compiler's score, from the row — never derived on the page. */
  confidence: number
  version: number
  updatedAt: string
}

/** One compiler run. Engineer mode shows these; simple mode never does. */
export interface CompilerRun {
  id: string
  startedAt: string
  durationSeconds: number
  processor: Processor
  filesProcessed: number
  candidates: number
  accepted: number
  /** Failed the mechanical span check — the number that matters. */
  rejectedUnsupported: number
  rejectedDuplicate: number
  stage: "structural" | "candidates" | "consolidation" | "validation" | "done"
}

export interface Company {
  name: string
  initials: string
  employees: string
  industry: string
}

/**
 * One AI application on this machine, as the daemon sees it.
 *
 * `state` rather than a boolean because "Connect" hides the four situations
 * an owner is actually in, and only one of them is alarming. An application
 * that is connected to a Knowlith which no longer exists looks identical to
 * a working one unless the difference is named.
 */
export interface AiTool {
  slug: "claude-desktop" | "claude-code" | "codex" | "cursor"
  label: string
  state: "missing" | "ready" | "connected" | "needs-attention"
  installed: boolean
  connected: boolean
  /** Whether the application has a window open right now. */
  running: boolean
  /**
   * How "try" / "ask" reaches this tool on this machine.
   * Desktop apps open outside; CLIs open a Terminal session.
   */
  launchSurface: "desktop" | "terminal" | "missing"
  configPath: string | null
  needsRestart: boolean
  refreshHint: string
  /** Set when something is wrong that the owner can repair. */
  problem: string | null
  /**
   * Distinct things this application has read.
   *
   * Proof of use, which is a different claim from proof of connection: a
   * configuration file with our entry in it says somebody pressed a
   * button, this says the company's own knowledge reached a conversation.
   */
  reads: number
  /** When it last read something, or null if it never has. */
  lastRead: string | null
}

/** Approved knowledge as a live graph the owner can walk. */
export interface CompanyBrain {
  nodes: BrainNode[]
  edges: BrainEdge[]
  assistants: BrainAssistant[]
}

export interface BrainNode {
  id: string
  title: string
  kind: string
  status: string
}

export interface BrainEdge {
  from: string
  to: string
  type: string
}

export interface BrainAssistant {
  slug: string
  label: string
  surface: "desktop" | "terminal" | "missing"
}

/** What connecting would write, shown before it is written. */
export interface ConnectPreview {
  configPath: string | null
  snippet: string
  refreshHint: string
}

/** What the background service is allowed to do on its own. */
export interface Policy {
  processing: "automatic" | "ask" | "manual"
  pauseOnBattery: boolean
  largeScan: number
  /** Which CLI reads documents: auto | codex | claude-code | cursor-agent | managed */
  engine: string
  /** Parallel CLI compile workers (I/O track is separate). Default 2. */
  compileWorkers?: number
  /** Documents packed into one CLI invoke. Default 8. */
  compileBatchSize?: number
}

export interface HeldWork {
  kind: string
  reason: string
  count: number
}

export interface PolicyState {
  policy: Policy
  held: HeldWork[]
  onBattery: boolean
  /** Present when the saved engine changed — worker still holds the old one. */
  engineRestart?: string | null
}

export interface DetectedEngine {
  id: string
  label: string
  program: string
  installed: boolean
  path: string | null
  version: string | null
}

/** Whether the daemon starts with the machine. */
export interface AutostartState {
  enabled: boolean
  location: string | null
  running: boolean
}

/**
 * What is in a folder, counted without reading any of it.
 *
 * Shown before the owner commits, because "412 files, 38 MB, 210 of them
 * PDFs" is the answer to a question they are entitled to ask first.
 */
export type Inventory = {
  files: number
  readable: number
  bytes: number
  types: { label: string; count: number; bytes: number }[]
  /** Types that were found and cannot be read, named rather than summed. */
  skipped: { label: string; count: number }[]
  duplicates: number
  oldVersions: number
  /** When the most recently changed file was last written. */
  newest: string | null
  truncated: boolean
}

/** A folder the owner picked, or `chosen: null` when they closed the chooser. */
export type Browsed = {
  chosen: string | null
  name: string | null
  inventory: Inventory | null
}

export type SourceAdded = {
  id: string
  name: string
  path: string
  queued: boolean
  alreadyKnown: boolean
}

/**
 * The daemon in four numbers.
 *
 * The two counts matter as much as the two queue figures: work can start and
 * finish between two polls, and then the queue never looks busy. What
 * changed is what the screens watch.
 */
export type Health = {
  queued: number
  working: number
  documents: number
  objects: number
}

/**
 * One thing an AI tool did with this company's knowledge.
 *
 * A row is a question, not a protocol call. An agent putting a quotation
 * together makes five or six calls; the owner should see one thing that
 * happened, with what it read underneath.
 */
export type Usage = {
  id: string
  app: "claude-desktop" | "claude-code" | "codex" | "cursor" | null
  /** "Another tool" when the client did not identify itself as one we know. */
  appLabel: string
  /** What the agent said it was doing, when it declared a case. */
  question: string | null
  at: string
  read: UsedObject[]
  /**
   * Named as relevant and never opened. Nothing else can report this: it
   * only exists because the approved set is finite and the gateway said
   * out loud what touched the question before the agent chose.
   */
  skipped: UsedObject[]
  /** False when the agent answered without checking what it had missed. */
  closed: boolean
}

export type UsedObject = {
  id: string
  title: string
  kind: "rule" | "process" | "term" | "skill" | "fact"
}

/**
 * What Knowlith is doing right now.
 *
 * `done` and `total` count the current burst of work, not the whole
 * history — adding a folder starts the bar at nought rather than at
 * whatever last week's jobs would make it. There is deliberately no
 * estimated time: a document takes anywhere from a second to a minute
 * depending on its size and the engine, and the first run has no history
 * to predict from, so any number would be invented.
 */
export type Work = {
  stage: "reading" | "thinking" | "preparing" | "held" | "idle"
  doing: string
  done: number
  total: number
  held: { reason: string; count: number } | null
  lines: WorkLine[]
}

export type WorkLine = {
  /** The document's name, never its id. */
  subject: string
  /** What the job did, in its own words: "14 claims · 2 not read". */
  note: string
  state: "done" | "failed" | "working"
  at: string
}
