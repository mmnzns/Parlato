// Parlato: a status line under a control is plain text, or a failure.
// Failures are tagged instead of being recognised by their translated
// "Error:" prefix, which broke as soon as a wording changed.

export type Failure = { failed: string };
export type Status = string | Failure;

export const fail = (text: string): Failure => ({ failed: text });

export const isFailure = (s: Status | null | undefined): s is Failure =>
  typeof s === "object" && s !== null;

export const statusText = (s: Status): string => (typeof s === "string" ? s : s.failed);
