# Lean Formal Verification Layer

This document describes the Lean 4 formal verification layer for pgrsql's query optimizer. The verification layer provides mathematical proofs that query rewrite rules preserve semantics, ensuring that optimized queries always produce the same results as the originals.

## Why Lean?

SQL optimizers rely on rewrite rules (predicate pushdown, join reordering, filter merge, etc.) that are typically validated through testing and fuzzing. However, SQL semantics are subtle — NULL propagation, three-valued logic, outer join behavior, and aggregation edge cases can cause silent correctness bugs that tests miss.

**Lean 4** is both a theorem prover and a dependently typed programming language. It allows us to:

- **Define formal SQL semantics** as algebraic data types
- **State equivalence theorems** for every rewrite rule
- **Machine-check proofs** that transformations are correct for *all possible inputs*
- **Encode NULL/three-valued logic** and prove properties under real SQL semantics

This transforms optimizer correctness from a testing problem into a mathematical guarantee:

```
∀ databases D, eval(original_query, D) = eval(rewritten_query, D)
```

### Why not Coq, Agda, or Isabelle?

| Criterion | Lean 4 | Coq | Agda | Isabelle |
|-----------|--------|-----|------|----------|
| Native code compilation | Yes | Limited | Limited | No |
| Meta-programming | Powerful macro system | Ltac/Ltac2 | Reflection | Isar |
| Learning curve | Moderate | Steep | Steep | Steep |
| Active development | Very active | Mature | Niche | Mature |
| Package manager (Lake) | Built-in | opam | cabal | Session |

Lean 4's combination of a modern type system, native compilation, and active ecosystem makes it the best fit for integrating formal verification into a Rust systems project.

## Architecture

```
pgrsql/
├── src/                          # Rust execution engine
│   ├── ast/
│   │   ├── optimizer.rs          # Runtime optimizer (Rust)
│   │   ├── parser.rs             # SQL → AST parsing
│   │   └── compiler.rs           # AST → SQL compilation
│   └── ...
└── lean/                         # Formal verification layer (Lean 4)
    ├── lakefile.toml             # Lake build configuration
    ├── lean-toolchain            # Pinned Lean version
    └── Verification/
        ├── Basic.lean            # Core types: Value, Tuple, Relation, TVL
        ├── Operations.lean       # Relational algebra operators
        ├── Theorems.lean         # Verified rewrite rule proofs
        ├── NullSemantics.lean    # Three-valued logic formalization
        └── Examples.lean         # Executable examples and tests
```

**Lean acts as the proof authority. Rust remains the execution engine.**

The Lean layer formally specifies relational algebra semantics and proves that specific transformations preserve query equivalence. The Rust optimizer implements the same rules for runtime execution. CI ensures both layers stay in sync.

## Module Overview

### `Verification.Basic`

Core type definitions modeling a subset of PostgreSQL:

- **`Value`**: SQL values with NULL support (`null | bool | int | string`)
- **`Tuple`**: Row as a list of `(column_name, value)` pairs
- **`Relation`**: Schema (column names) + list of tuples
- **`TVL`**: Three-valued logic (`true | false | unknown`)
- **`Predicate`**: Function from tuple to TVL (models WHERE clauses)
- Helper functions: `lookupColumn`, `projectTuple`, `mergeTuples`
- TVL operators: `and`, `or`, `not`, `isTrue` (with full SQL semantics)

### `Verification.Operations`

Relational algebra operators:

| Operator | Function | SQL Equivalent |
|----------|----------|----------------|
| Selection (σ) | `select p r` | `SELECT * FROM r WHERE p` |
| Projection (π) | `project cols r` | `SELECT cols FROM r` |
| Cross Product (×) | `crossProduct r s` | `FROM r, s` |
| Theta Join (⨝) | `join p r s` | `FROM r JOIN s ON p` |
| Rename (ρ) | `rename old new r` | `AS` aliasing |
| Union (∪) | `union r s` | `UNION ALL` |
| Intersection (∩) | `intersection r s` | `INTERSECT` |
| Difference (−) | `difference r s` | `EXCEPT` |

Also defines compound predicates (`predAnd`, `predOr`, `predNot`) and a `RelExpr` inductive type for representing query plans as data with an `eval` function.

### `Verification.Theorems`

Machine-checked proofs of optimizer rewrite rules:

| Theorem | Statement | Optimizer Rule |
|---------|-----------|----------------|
| `filter_merge` | σ_c(σ_d(R)) = σ_(c∧d)(R) | Merge consecutive WHERE clauses |
| `select_comm` | σ_c(σ_d(R)) = σ_d(σ_c(R)) | Reorder filter predicates |
| `select_idempotent` | σ_c(σ_c(R)) = σ_c(R) | Eliminate duplicate filters |
| `select_true` | σ_TRUE(R) = R | Eliminate tautological filters |
| `select_false` | σ_FALSE(R) = ∅ | Eliminate contradictory filters |
| `select_union_dist` | σ_c(R ∪ S) = σ_c(R) ∪ σ_c(S) | Push predicates through UNION |
| `union_assoc` | (R ∪ S) ∪ T = R ∪ (S ∪ T) | Union associativity |
| `cross_empty_right` | R × ∅ = ∅ | Empty relation elimination |
| `cross_empty_left` | ∅ × S = ∅ | Empty relation elimination |
| `join_is_select_cross` | R ⨝_p S = σ_p(R × S) | Join decomposition |
| `predAnd_comm` | (p ∧ q)(t) = (q ∧ p)(t) | Predicate commutativity |
| `predOr_comm` | (p ∨ q)(t) = (q ∨ p)(t) | Predicate commutativity |
| `tvl_demorgan_and` | ¬(a ∧ b) = (¬a) ∨ (¬b) | De Morgan's law for 3VL |
| `tvl_demorgan_or` | ¬(a ∨ b) = (¬a) ∧ (¬b) | De Morgan's law for 3VL |
| `tvl_not_not` | ¬¬v = v (for definite v) | Double negation elimination |
| `project_idempotent` | π_A(π_A(R)) = π_A(R) | Eliminate redundant projections (Phase 2) |

### `Verification.NullSemantics`

Formal verification of SQL's three-valued logic:

- **Truth table verification**: AND, OR, NOT truth tables match SQL standard
- **NULL propagation**: `AND` with UNKNOWN never produces TRUE
- **Absorption laws**: `OR TRUE _ = TRUE`, `AND FALSE _ = FALSE`
- **Algebraic properties**: associativity, commutativity, distributivity
- **De Morgan's laws**: Verified for all 27 input combinations (3^3)

### `Verification.Examples`

Executable examples that serve as integration tests:

- Concrete relation definitions (employees, departments)
- Selection, projection, cross product examples
- Filter merge verification on concrete data
- NULL handling demonstration (UNKNOWN filtered correctly)
- All validated with `native_decide` (computed and checked at compile time)

## Prerequisites

### Install Lean 4

The recommended way to install Lean 4 is via [elan](https://github.com/leanprover/elan), the Lean version manager:

```bash
# Linux / macOS
curl https://elan.lean-lang.org/elan-init.sh -sSf | sh

# Follow the prompts, then restart your shell or run:
source ~/.profile
```

The `lean-toolchain` file in the `lean/` directory pins the exact Lean version used by this project. `elan` will automatically install the correct version when you build.

### Verify installation

```bash
lean --version
# Lean (version 4.28.0, ...)

lake --version
# Lake version 5.0.0-...
```

## Building & Verifying Proofs

All commands are run from the `lean/` directory:

```bash
cd lean/

# Build the project and verify all proofs
lake build

# Clean build artifacts
lake clean

# Rebuild from scratch
lake clean && lake build
```

A successful `lake build` means **all proofs are machine-checked**. If any proof is invalid, the build will fail with a type error.

### Expected output

```
⚠ [6/8] Built Verification.Theorems
warning: Verification/Theorems.lean:146:8: declaration uses `sorry`
✔ [7/8] Built Verification
Build completed successfully (8 jobs).
```

The `sorry` warning is expected — it marks `project_idempotent` as an incomplete proof targeted for Phase 2. All other theorems are fully verified.

## Running Tests

The Lean verification layer uses two complementary testing strategies:

### 1. Proof Verification (Primary)

```bash
cd lean/ && lake build
```

Every `theorem` in `Theorems.lean` and `NullSemantics.lean` is a machine-checked proof. If the build succeeds, all proofs are valid. There is no separate "test run" — **the build IS the test**.

### 2. Executable Examples

The `Examples.lean` file contains `example` declarations that use `native_decide` to compute and verify concrete results at compile time:

```lean
-- This is checked at compile time: if wrong, the build fails
example : (select isEng employees).tuples.length = 2 := by native_decide
```

These serve as sanity checks that the formal definitions match expected behavior on concrete data.

### 3. Checking for Incomplete Proofs

```bash
# List all sorry usages (incomplete proofs)
grep -rn "sorry" lean/Verification/ --include="*.lean"
```

The goal is to reduce `sorry` count to zero over time.

### 4. Rust Test Suite

The existing Rust tests continue to validate runtime behavior:

```bash
cargo test
```

## CI Integration

The `.github/workflows/lean.yml` workflow runs on every push and PR that modifies files in `lean/`:

1. **Build & Verify Proofs**: Installs elan, runs `lake build`, verifies all proofs typecheck
2. **Lean Lint**: Checks project structure and reports `sorry` usage

This runs alongside the existing Rust CI pipeline (`ci.yml`). Both must pass for PRs to merge.

## Future Implementation Roadmap

### Phase 1 (Current): Formal Relational Algebra Core

- [x] Define Value, Tuple, Relation types
- [x] Define three-valued logic (TVL) with SQL semantics
- [x] Implement relational algebra operations (σ, π, ⨝, ρ, ∪, ∩, −)
- [x] Prove filter merge correctness
- [x] Prove selection commutativity, idempotence
- [x] Prove predicate pushdown through UNION
- [x] Prove De Morgan's laws for 3VL
- [x] Verify NULL propagation properties
- [x] Establish CI proof verification
- [ ] Complete `project_idempotent` structural proof

### Phase 2: Verified Rewrite Rules

- [ ] Prove projection pushdown through selection
- [ ] Prove join associativity (under safe conditions)
- [ ] Prove CTE inlining equivalence
- [ ] Formalize aggregation operators (γ)
- [ ] Prove GROUP BY / HAVING interactions
- [ ] Window function normalization equivalence
- [ ] Add Mathlib dependency for advanced algebraic reasoning

### Phase 3: Rust-Lean Integration

- [ ] Define canonical query IR shared by Lean proofs and Rust optimizer
- [ ] Generate verified rewrite rule definitions from Lean
- [ ] CI consistency checks between Lean specifications and Rust implementations
- [ ] Proof certificates for optimizer rule application

### Phase 4: NULL and Three-Valued Logic Deep Formalization

- [ ] Full outer join semantics (LEFT, RIGHT, FULL)
- [ ] Aggregation behavior with NULL (COUNT vs SUM edge cases)
- [ ] COALESCE and NULL-handling function semantics
- [ ] IS DISTINCT FROM operator verification

### Phase 5: Advanced Verification

- [ ] Verified cost-based optimizer foundations
- [ ] Subquery decorrelation proofs
- [ ] Materialized view equivalence
- [ ] Multi-language query equivalence (SQL, Python DSL, OCaml)

## Contributing

### Adding a New Theorem

1. State the theorem in `Verification/Theorems.lean`:
   ```lean
   theorem my_new_rule (r : Relation) :
       some_transform r = equivalent_transform r := by
     sorry -- TODO: prove this
   ```

2. Build to verify the statement typechecks:
   ```bash
   lake build
   ```

3. Replace `sorry` with a proof. Common tactics:
   - `rfl` — definitional equality
   - `simp` — simplification
   - `cases x <;> rfl` — case analysis on finite types (great for TVL)
   - `congr 1` — prove structure equality field by field
   - `funext t` — prove function equality pointwise
   - `induction l with ...` — structural induction on lists
   - `native_decide` — compute and check concrete examples

4. Build again to verify the proof is accepted.

### Adding a New Operation

1. Define the operation in `Verification/Operations.lean`
2. Add examples in `Verification/Examples.lean`
3. State and prove relevant properties in `Verification/Theorems.lean`
4. Update imports in `Verification.lean` if adding new files

### Project Conventions

- One `sorry` per incomplete proof, with a comment explaining the plan
- All definitions should have doc comments (`/-- ... -/`)
- Use `section` / `namespace` to organize related definitions
- Prefer `cases a <;> cases b <;> rfl` for TVL proofs (exhaustive case analysis)
- Use `native_decide` in `Examples.lean` for concrete validation

## References

- [Lean 4 Documentation](https://lean-lang.org/lean4/doc/)
- [Lean 4 Theorem Proving Tutorial](https://lean-lang.org/theorem_proving_in_lean4/)
- [Functional Programming in Lean](https://lean-lang.org/functional_programming_in_lean/)
- [Mathlib4](https://github.com/leanprover-community/mathlib4) — Mathematical library for Lean 4
- Codd, E.F. "A Relational Model of Data for Large Shared Data Banks" (1970)
- Date, C.J. "SQL and Relational Theory" (3rd ed.)
