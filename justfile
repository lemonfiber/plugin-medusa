# Task runner for lemonfiber/plugin-medusa. `just` with no argument lists these.
default:
    @just --list

# Turn on the repository's own git hooks. Once per clone.
hooks:
    git config core.hooksPath .githooks
    @echo "hooks on: .githooks/commit-msg, .githooks/pre-push"

# The gates that need nothing but this tree, in CI's order. The plugin verdict
# (spec's first-party-plugin workflow) and the conformance recordings, which
# need Docker, run in CI; `just recordings` is the second of them.
ci: hooks fmt clippy test coverage deny typos links

# The adapter's format.
fmt:
    cargo fmt --check

# The adapter's lints, every warning an error.
clippy:
    cargo clippy --all-targets --locked -- -D warnings

# The adapter's tests.
test:
    cargo test --locked

# Every line of the adapter the tests can reach, reached.
coverage:
    cargo llvm-cov --locked --ignore-filename-regex "$(cat .config/coverage-skipped)" --fail-under-lines 100

# The adapter's dependencies, held to `deny.toml`.
deny:
    cargo deny --log-level warn check

# Every recording, taken again with the adapter built from this tree and compared.
# Needs Docker, and `lemonfiber` at the commit `targets.toml` names on the path.
recordings:
    docker build -q -t plugin-medusa:pr .
    python3 -c 'import tomllib; print("\n".join(s["image"] + "@" + s["digest"] for s in tomllib.load(open("plugin.toml", "rb"))["service"] if "fronts" not in s))' | xargs -n1 docker pull -q
    lemonfiber plugin record . --check --adapter plugin-medusa:pr

# Spell check.
typos:
    typos

# Link check.
links:
    lychee --no-progress .
