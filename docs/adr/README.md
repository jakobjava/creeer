# Architecture decision records

One file per decision, numbered in order (`NNNN-short-name.md`). A decision that changes later is amended in its own file ("Amended in …"), with the date and the reason, and never silently rewritten.

## What every ADR contains

1. **Status, context, builds on, evidence.** The header: whether it is proposed, in progress or accepted; what asked for it; the ADRs it rests on; where its code, tests and benchmarks are.
2. **Prior work** (required, from 2026-10-18; plan.md §1, principle 12). Before the design: how others have solved the problem, with sources.
   - **Where to look:** papers (SIGGRAPH, ACM TOG, Eurographics, arXiv), other engines' documentation and source (PhysX, MuJoCo, Havok, Jolt, Bullet, Box2D, Godot, Unreal), shipped games and their talks (GDC), standards and handbooks for physical data.
   - **What to record:** each relevant method in a line or two (what it does, its reported results and limits), and what it means for Creëer: taken, adapted, or why not. A table with sources is the usual form.
   - **What decides:** the design starts from what the survey found. A method is built from first principles only where the survey shows nothing fits, and the ADR says so.
   - A survey that finds nothing is recorded too, with what was searched.
3. **Decision.** What was built, and why, with the numbers that decided it.
4. **Measured and rejected.** Every variant tried and not taken: what it was, what it measured, why it was rejected. Kept patches go beside the ADR (`docs/adr/NNNN/`), since session scratch space does not last.
5. **Validation.** The tests and their results against analytic or published references.
6. **Cost.** Its budget and what it measures, on the development machine.
7. **Limits.** What it does not do, each also in `docs/future-improvements.md`.

## Why the survey is required

P1 (ADR-0024) spent weeks on methods the literature had already settled. The ragdolls' slowness had a known answer, reduced coordinates with maximal-coordinate constraints only where loops close (RedMax, 2019). ADR-0022 had rejected reduced coordinates for closed loops without looking. The method P1 planned, a group split across cores, was shown by published results not to pay for groups of tens of bodies. An hour's search would have found both.

ADRs written before 2026-10-18 have no Prior work section. When one of them is amended, or its subject comes up again, its survey is done then and added.
