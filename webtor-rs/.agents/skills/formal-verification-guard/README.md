# Formal Verification Guard

A Claude Agent Skill that protects against hallucinated scientific breakthroughs and LLM-assisted self-deception.

## Problem

LLMs are increasingly used to develop scientific and mathematical ideas. However, their sycophantic tendencies can trap users into believing they've made genuine breakthroughs when they haven't. This skill implements systematic defenses based on [this LessWrong analysis](https://www.lesswrong.com/posts/rarcxjGp47dcHftCP/your-llm-assisted-scientific-breakthrough-probably-isn-t).

## How It Works

```
+------------------+     +-------------------+     +------------------+
|  Trigger Detection |---->|  Skeptical Mode   |---->|  Claim Dossier   |
|  - Long convos     |     |  - Extract claims |     |  - Track status  |
|  - Hype language   |     |  - Adversarial    |     |  - Log evidence  |
|  - Crank keywords  |     |    critique       |     |  - External refs |
+------------------+     +-------------------+     +------------------+
```

When activated, the skill:

1. **Triages** - Detects red flags (long conversations, praise patterns, crank keywords)
2. **Extracts** - Formalizes vague claims into precise, inspectable statements
3. **Enforces falsifiability** - Demands testable predictions or formal propositions
4. **Checks prior art** - Assumes ideas are rediscoveries until proven otherwise
5. **Runs adversarial critique** - Multiple skeptical personas attack the idea
6. **Performs deterministic checks** - Runs code, searches for counterexamples
7. **Maintains provenance** - Tracks evidence chains in a Claim Dossier
8. **Blocks sycophancy** - Refuses to hype unverified claims

## Installation

Copy the `formal-verification-guard/` directory to your skills location:

```bash
# For Claude Code
cp -r formal-verification-guard ~/.claude/skills/

# Or for project-specific
cp -r formal-verification-guard .claude/skills/
```

## Structure

```
formal-verification-guard/
+-- SKILL.md                 # Main skill instructions
+-- README.md                # This file
+-- checklists.md            # Red flag triage, falsifiability checks
+-- adversarial_prompts.md   # Skeptical reviewer personas
+-- templates/
|   +-- claim_dossier.md     # Template for tracking claims
+-- scripts/
    +-- check_numeric_claim.py      # Numeric sanity checks
    +-- run_verification_plan.py    # Batch verification runner
```

## Key Features

| Failure Mode | Countermeasure |
|--------------|----------------|
| Sycophancy traps | Anti-sycophancy rules, enthusiasm = zero evidence |
| Long context bias | Triggers at >8 turns, flips to skeptical mode |
| No falsifiability | Forces precise statements, testable predictions |
| Missing peer review | Adversarial personas + independent model checks |
| Crank keywords | Red flag detection for "quantum", "fractal", "emergent" abuse |
| No evidence tracking | Claim Dossier with provenance and status transitions |

## Disclaimer

This skill is itself an **unvalidated engineering synthesis**, not a proven safety intervention. By its own standards:

- **Evidence level**: Conceptual argument only
- **Status**: Promising but empirically untested
- **Classification**: Engineering workflow / tool

Use it as a helpful framework, not a guarantee.

## References

- [Your LLM-assisted scientific breakthrough probably isn't real](https://www.lesswrong.com/posts/rarcxjGp47dcHftCP/your-llm-assisted-scientific-breakthrough-probably-isn-t) - LessWrong
- [Claude Agent Skills Documentation](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview)
