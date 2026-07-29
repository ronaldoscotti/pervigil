/** The shapes the Rust side serializes. */

export type SessionState = "Working" | "WaitingOnYou" | "YourTurn" | "Idle";
export type Span = "4h" | "today" | "week";

export interface SessionView {
  id: string;
  project: string;
  name: string;
  branch: string | null;
  state: SessionState;
  since: number;
  siblings: number;
  cost: number | null;
  focus: string;
  pinned: boolean;
}

export interface FocusOutcome {
  raised: boolean;
  label: string;
  resume: string | null;
  error: string | null;
}

export interface Segment {
  state: SessionState;
  from: number;
  to: number;
}

export interface Snapshot {
  now: number;
  from: number;
  waiting: number;
  waitingOutsideWindow: number;
  sessions: SessionView[];
  segments: Segment[];
  waitingShare: number;
  cost: number;
  tokens: number;
  notifications: boolean;
  dismissRead: boolean;
  hidden: string[];
  hooksInstalled: boolean;
  hookSnippet: string;
}
