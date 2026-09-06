# ADR 0028: OCR starts from a committed selection

- Status: Accepted
- Date: 2026-09-06

## Problem

A render effect started OCR whenever a valid selection existed in OCR mode.
The first small pointer movement therefore prepared a tiny crop before the
user finished dragging. It could also race the pointer-release action and
issue a duplicate preparation request.

## Decision

Remove render-driven OCR. A new OCR region is committed on pointer release,
using the normalized start and final pointer position rather than a potentially
stale selection render. A plain click does not recognize. Explicitly switching
a completed screenshot region to OCR prepares that existing region once.

Keep selection movement/resizing, local OCR, remote-provider consent, request
cancellation and native capture boundaries unchanged. The recognized-text
surface remains scrollable and gains a labelled keyboard-focusable region.

## Verification

Regression tests exercise slow partial drags without a request, one full crop
on release, plain clicks, reverse-direction selection, and reuse of a completed
selection. The documentation capture asserts the actual IPC crop and request
count instead of trusting its planned mouse coordinates. Browser checks use
the production frontend with isolated documentation IPC, not a native OCR
engine or real provider request.
