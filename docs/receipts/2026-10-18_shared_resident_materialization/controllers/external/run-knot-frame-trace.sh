#!/bin/zsh
# Knot mere overmap live receipt: a fixture resident, two Djinn-attached Knot
# processes (seed, then a fresh-process reopen), then a standalone Knot with
# no resident. Every process it starts is stopped before it returns.
#
# run_overmap_live.sh <knot> <knot_overmap_live_host> <knot-checkout> <mere-checkout> <new-out-dir> <doc-name>...
# Documents are named relative to the fixture root, e.g. essay.djot.
set -eu
KNOT=$1 HOST=$2 KNOT_SRC=$3 MERE_SRC=$4 OUT=$5
shift 5
export TMPDIR=/tmp
PERSONA=00000000-0000-0000-6b6e-6f7420261015
ENDPOINT=/tmp/kov-$(uuidgen | cut -c1-8 | tr A-Z a-z).sock
[[ ! -e $OUT ]] || { echo "output must be new: $OUT"; exit 2; }
mkdir -p $OUT/logs
echo $ENDPOINT > $OUT/endpoint.txt
scenario() { # <scenario> <output> <profile> <knot args...>
  local scn=$1 out=$2 profile=$3; shift 3
  python3 $MERE_SRC/scripts/run_macos_scenario.py --binary $KNOT --prefix KNOT --timeout 90 \
    --scenario $KNOT_SRC/scenarios/$scn --output $OUT/$out \
    --env CAMBIUM_HOST_FRAME_TRACE=1 --env CAMBIUM_HOST_PERF_TRACE=1 --env KNOT_TEST_INITIAL_PRESENTATION_GRACE_SECS=60 --env TMPDIR=/tmp \
    --env APPDATA=$OUT/$profile --env GRAPHSHELL_APP_ENDPOINT=$ENDPOINT \
    -- "$@" > $OUT/logs/$out.log 2>&1 || echo "$out: runner exit $?"
}

# 1. The fixture resident: public test keys in a new marked root.
mkfifo $OUT/host.stdin
$HOST $ENDPOINT $OUT/fixture < $OUT/host.stdin > $OUT/logs/fixture-host.log 2>&1 &
HOST_PID=$!
exec 3> $OUT/host.stdin
stopped=0
stop_host() {
  (( stopped )) && return; stopped=1
  echo stop >&3 || true; exec 3>&-
  wait $HOST_PID; echo "host exit=$?" >> $OUT/logs/fixture-host.log
}
trap stop_host EXIT
for i in {1..300}; do [[ -e $OUT/fixture/ready.json ]] && break; sleep 0.1; done
[[ -e $OUT/fixture/ready.json ]] || { echo "fixture not ready"; exit 1; }

# 2. The fixture root is the catalog root and holds the marker, so Knot's
#    marked-fixture startup grace applies. Knot catalogues each document it
#    opens; a file it never opens is not in the catalog.
DOCS=(); for name in "$@"; do DOCS+=($OUT/fixture/$name); done
LIVE=(--catalog-root $OUT/fixture --catalog $OUT/live-catalog.redb
      --mere-root $OUT/fixture/shared --mere-persona $PERSONA --mere-domain divination)
scenario overmap_live.scn live-native live-profile $LIVE $DOCS
scenario overmap_live_reopen.scn live-native-reopen live-profile $LIVE $DOCS
stop_host

# 3. Standalone: no resident and no --mere-* options; own profile and catalog.
scenario overmap_standalone.scn standalone standalone-profile \
  --catalog-root $OUT/fixture --catalog $OUT/standalone-catalog.redb $DOCS
for d in live-native live-native-reopen standalone; do
  echo "$d: $(head -1 $OUT/$d/scenario.done 2>/dev/null || echo missing)"
done
