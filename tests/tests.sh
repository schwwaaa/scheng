#bin/bash

cargo metadata --no-deps --format-version=1 --quiet \
| jq -r '
  (.workspace_members | map(.) ) as $ws
  | .packages[]
  | select(.id as $id | $ws | index($id))
  | .name as $pkg
  | .targets[]
  | select((.kind|index("example")) or (.kind|index("bin")))
  | if (.kind|index("example")) then
      "cargo run -p \($pkg) --example \(.name)"
    else
      "cargo run -p \($pkg) --bin \(.name)"
    end
' | sort



# cargo run -p scheng-example-feedback-orb --bin scheng-example-feedback-orb
# cargo run -p scheng-example-feedback-pingpong --bin scheng-example-feedback-pingpong
# cargo run -p scheng-example-graph-chain2 --bin scheng-example-graph-chain2
# cargo run -p scheng-example-graph-matrix-mix4 --bin scheng-example-graph-matrix-mix4
# cargo run -p scheng-example-graph-minimal --bin scheng-example-graph-minimal
# cargo run -p scheng-example-graph-mixer-builtin --bin scheng-example-graph-mixer-builtin
# cargo run -p scheng-example-graph-mixer2 --bin scheng-example-graph-mixer2
# cargo run -p scheng-example-minimal --bin scheng-example-minimal
# cargo run -p scheng-example-pure-single-pass --bin scheng-example-pure-single-pass
# cargo run -p scheng-example-render-target-only --bin scheng-example-render-target-only
# cargo run -p scheng-example-temporal-slitscan --bin scheng-example-temporal-slitscan