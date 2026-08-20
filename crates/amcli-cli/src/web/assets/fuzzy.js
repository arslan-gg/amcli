// One matcher, behind every search box on the page.
//
// Substring matching made you spell a name the way the file spells it. In a
// model whose component is called "Customer Onboarding API", `cust api` found
// nothing, `onboardng` found nothing, and `CustomerOnboarding` found nothing —
// each of them a reader who knew exactly which box they wanted. What someone
// has in mind is the letters, roughly in order; that is what this matches, and
// the ranking is what keeps the exact answer on top when there is one.
//
// `rank` is the one currency: -1 is no match, and lower is better. The four
// tiers do not overlap, so a scattered hit can never outrank a solid one.
//
//   0        the whole name
//   [1, 2)   a prefix
//   [2, 3)   a substring — the earlier it starts, the better
//   [3, 4)   the letters in order — the tighter and the more word-initial,
//            the better
//
// Whitespace in what is typed is dropped rather than matched: a space is how
// a reader separates the parts of a name they half-remember, and requiring it
// to appear in the same place in the file is the substring rule again by
// another route.

import { h } from "./dom.js";

// The penalties of the fourth tier, in one currency. A run of letters is what
// a word looks like, so continuing one costs nothing; jumping costs, unless
// the jump lands where a word starts, which is what a reader means by typing
// initials. Every jump still costs something — LOOSE + GAP exceeds BOUNDARY —
// or `cmp` on "Component" would tie with "C… M… P…".
const LEAD = 0.02;     // per character before the first match
const LOOSE = 0.5;     // a match that does not continue the one before it
const GAP = 0.05;      // per character skipped, to a point
const GAP_CAP = 8;     // past which a gap is just "elsewhere in the name"
const BOUNDARY = 0.45; // off a jump that lands on the first letter of a word
const SLIP = 2;        // letters a typo may drop without leaving its word

const WORD = /[\p{L}\p{N}]/u;

// Where the fourth tier's bonus applies: the start, the letter after anything
// that is not a letter or a digit, and the capital in `dataStore`. The class
// is unicode-aware because a model may be written in any script — `[^a-z0-9]`
// made every Cyrillic letter a word boundary and scored noise as intent.
function startsWord(text, i) {
  if (i === 0) return true;
  const prev = text[i - 1];
  if (!WORD.test(prev)) return true;
  return prev === prev.toLowerCase() && text[i] !== text[i].toLowerCase();
}

// Scattered letters are cheap to find and worthless to read: on a real model
// `data` matched five names as a substring and thirty-three as a subsequence,
// the other twenty-eight being names like "Nodal GROs at RE and LSP", where a
// d, an a, a t and an a happen to fall in that order. A table sorted by name
// gives noise the same standing as the answer, so the fourth tier accepts only
// what a reader could have meant:
//
//   · the match starts a word — "cnsnt" starts at the C of "Consent", and
//     "sent" is a substring, which is matched a tier above;
//   · every letter after it continues the one before, starts another word
//     ("cbs" → "Core Banking System"), or drops at most two letters without
//     leaving the word it is in, which is what a typo looks like
//     ("aplication" → "Application", "onboardng" → "Onboarding").
//
// A jump that steps over the start of a word and lands inside the next one is
// the one thing refused, and it is what all twenty-eight were.
function accepts(at, text) {
  if (!startsWord(text, at[0])) return false;
  for (let j = 1; j < at.length; j++) {
    const gap = at[j] - at[j - 1] - 1;
    if (gap === 0 || startsWord(text, at[j])) continue;
    if (gap > SLIP) return false;
    for (let i = at[j - 1] + 1; i < at[j]; i++) if (startsWord(text, i)) return false;
  }
  return true;
}

function penalty(at, text) {
  let p = at[0] * LEAD;
  for (let j = 1; j < at.length; j++) {
    const gap = at[j] - at[j - 1] - 1;
    if (gap === 0) continue;
    p += LOOSE + Math.min(gap, GAP_CAP) * GAP;
    if (startsWord(text, at[j])) p -= BOUNDARY;
  }
  return p;
}

// The same letters, each as late as it can be taken without passing the one
// after it. Matching forwards takes the first letter it sees, which in
// "Customer Onboarding API" spends the `a` of `cust api` on "onbo(a)rding"
// and then underlines it; pulling the letters back against the end finds the
// "API" the reader meant. Neither is always the better reading, so both are
// scored and the cheaper wins.
function packRight(at, needle, lower) {
  const out = at.slice();
  for (let k = out.length - 2; k >= 1; k--) {
    let i = out[k + 1] - 1;
    while (i > out[k] && lower[i] !== needle[k]) i--;
    out[k] = i;
  }
  return out;
}

// The best subsequence match, or null. Greedy from the earliest first letter
// decides whether the letters are there at all; the other first letters are
// then tried for a tighter match, and a name holds only a handful of them.
function scatter(needle, text, lower) {
  let best = null;
  let bestAt = null;
  const take = (at) => {
    if (!accepts(at, text)) return;
    const p = penalty(at, text);
    if (best === null || p < best) { best = p; bestAt = at; }
  };
  for (let s = 0; s < lower.length; s++) {
    if (lower[s] !== needle[0]) continue;
    const at = [s];
    let k = 1;
    for (let i = s + 1; i < lower.length && k < needle.length; i++) {
      if (lower[i] === needle[k]) { at.push(i); k++; }
    }
    // Greedy from here failed, so greedy from any later start fails too.
    if (k < needle.length) break;
    take(at);
    if (at.length > 2) take(packRight(at, needle, lower));
  }
  return bestAt === null ? null : { p: best, at: bestAt };
}

// The rank and the letters it matched, which is what a result list underlines.
export function probe(q, text) {
  const raw = (q || "").trim().toLowerCase();
  const body = text || "";
  if (!raw) return { r: -1, at: [] };
  const lower = body.toLowerCase();

  const run = (from, len, r) => ({ r, at: Array.from({ length: len }, (_, i) => from + i) });
  if (lower === raw) return run(0, raw.length, 0);
  if (lower.startsWith(raw)) return run(0, raw.length, 1);
  const at = lower.indexOf(raw);
  if (at >= 0) return run(at, raw.length, 2 + at / (at + 1000));

  const needle = raw.replace(/\s+/g, "");
  if (!needle) return { r: -1, at: [] };
  const found = scatter(needle, body, lower);
  if (!found) return { r: -1, at: [] };
  // Squashed into [0, 1), so no penalty however large can reach the next tier.
  return { r: 3 + found.p / (1 + found.p), at: found.at };
}

// Lower is better; -1 is no match.
export function rank(q, text) {
  return probe(q, text).r;
}

export function matches(q, text) {
  return probe(q, text).r >= 0;
}

// The name, with the letters the search matched marked. Runs are marked
// together, so a substring hit is one mark rather than nine.
export function mark(text, q) {
  const body = text || "";
  const { r, at } = probe(q, body);
  if (r < 0 || !at.length) return [body];
  const out = [];
  let cut = 0;
  for (let k = 0; k < at.length;) {
    let j = k;
    while (j + 1 < at.length && at[j + 1] === at[j] + 1) j++;
    if (at[k] > cut) out.push(body.slice(cut, at[k]));
    out.push(h("mark", null, body.slice(at[k], at[j] + 1)));
    cut = at[j] + 1;
    k = j + 1;
  }
  if (cut < body.length) out.push(body.slice(cut));
  return out;
}
