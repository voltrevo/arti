# Formal Verification Guard - Checklists

## 1. Red Flag Triage

Use this quick checklist when the user is excited about a "new idea":

- [ ] The conversation about this idea has lasted **> 8-10 turns**
      without external data, code, or citations.
- [ ] The assistant (or other models) have described it as
      "revolutionary", "paradigm-shifting", "world-changing", etc.
- [ ] The user reports that **multiple LLMs** agree it is a major breakthrough.
- [ ] The idea is mostly described with **vague or grandiose language**:
      - "explains everything", "unified theory of X", "complete solution to Y".
- [ ] The proposal leans heavily on seductive buzzwords:
      - "recursive", "fractal", "quantum", "emergent", "holographic",
        "field of consciousness", "torsion", "zero-point energy", etc.
- [ ] The idea conflicts with **strong, well-established results**
      (e.g., known complexity bounds, conservation laws) without addressing them.
- [ ] There is **no clear falsifiable prediction** or formal statement.
- [ ] There is **no concrete plan** for experiments, proofs, or benchmarks.
- [ ] The user is puzzled that experts / journals / communities
      are not already taking it seriously.

If **2+ boxes** are checked, treat as **Medium risk**.
If **4+ boxes** are checked, treat as **High risk** and apply the full workflow.

---

## 2. Claim Extraction Checklist

For each suspected "breakthrough":

- [ ] I have written a concise 1-2 sentence summary of the claim.
- [ ] I have classified it as:
      - [ ] Conceptual hypothesis
      - [ ] Formal theorem / proposition
      - [ ] Algorithmic claim
      - [ ] Empirical scientific claim
- [ ] I have identified all undefined or ambiguous terms.
- [ ] I have noted any implicit assumptions (e.g. ignoring noise, finite size).

---

## 3. Falsifiability Checklist

For **empirical claims**:

- [ ] Can I state the claim as:
      "If hypothesis H is true, then in experiment E with setup S and analysis A,
       we should observe outcome O (with magnitude M, confidence C)?"
- [ ] Does H make **different predictions** than standard models?
- [ ] Is there a **feasible experiment or dataset** that could show H is wrong?

For **mathematical / logical claims**:

- [ ] Is the domain of quantification explicitly stated?
- [ ] Are all objects (functions, sets, spaces, etc.) clearly defined?
- [ ] Are the assumptions minimal and explicit?
- [ ] Are there obvious **counterexamples** in small cases that can be checked?

For **algorithmic claims**:

- [ ] Is the input distribution specified?
- [ ] Is the claimed runtime / memory bound explicit?
- [ ] Are there baseline algorithms and metrics for comparison?
- [ ] Is there a plan to test on realistic data?

If these cannot be answered, the claim is **not yet scientific**.

---

## 4. Evidence-Level Classification

Classify each claim's current support:

- [ ] **Speculation / intuition** - no concrete evidence.
- [ ] **Conceptual argument** - informal reasoning, thought experiments.
- [ ] **Toy example / simulation** - works in small cases or simple simulations.
- [ ] **Pilot data / limited tests** - some empirical / benchmark evidence, but fragile.
- [ ] **Robust evidence / proof**:
      - Math: fully checked, no obvious gaps; ideally formalized.
      - Empirical: well-powered, well-controlled, reproducible experiments.
      - Algorithms: consistent gains across strong baselines and datasets.

Default classification should be **as low as is honestly defensible**.

---

## 5. Sycophancy Guardrails

When this Skill is active:

- [ ] I do **not** treat repeated user enthusiasm as evidence.
- [ ] I avoid mirroring phrases like "this is clearly a breakthrough" without strong evidence.
- [ ] I explicitly state when we're in "brainstorming / idea-generation mode"
      vs. "verification / reality-checking mode".
- [ ] I flag long, self-contained LLM conversations as **suspect context**
      rather than supportive evidence.
- [ ] I encourage contact with **human experts** or communities for external review.
