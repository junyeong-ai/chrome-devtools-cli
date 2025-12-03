#!/bin/bash
set -uo pipefail

CLI="./target/release/chrome-devtools-cli"
PASS=0
FAIL=0

red() { printf "\033[31m%s\033[0m" "$1"; }
green() { printf "\033[32m%s\033[0m" "$1"; }

run_test() {
    local name="$1"
    local cmd="$2"
    local expected="${3:-0}"

    if eval "$cmd" >/dev/null 2>&1; then
        if [ "$expected" -eq 0 ]; then
            green "✓"; echo " $name"
            PASS=$((PASS+1))
        else
            red "✗"; echo " $name (expected failure)"
            FAIL=$((FAIL+1))
        fi
    else
        if [ "$expected" -eq 1 ]; then
            green "✓"; echo " $name (expected failure)"
            PASS=$((PASS+1))
        else
            red "✗"; echo " $name"
            FAIL=$((FAIL+1))
        fi
    fi
}

check_json() {
    local name="$1"
    local cmd="$2"
    local output
    output=$(eval "$cmd" 2>/dev/null) || true

    if echo "$output" | jq . >/dev/null 2>&1; then
        green "✓"; echo " $name (valid JSON)"
        ((PASS++))
    else
        red "✗"; echo " $name (invalid JSON)"
        ((FAIL++))
    fi
}

cleanup() {
    $CLI stop >/dev/null 2>&1 || true
    pkill -f "chrome.*remote-debugging-port" 2>/dev/null || true
}

trap cleanup EXIT

echo "================================================"
echo "Chrome DevTools CLI Integration Tests"
echo "================================================"
echo ""

if [ ! -f "$CLI" ]; then
    echo "Error: $CLI not found. Run 'cargo build --release' first."
    exit 1
fi

cleanup
rm -f ~/.config/chrome-devtools-cli/chrome-profile/SingletonLock 2>/dev/null || true
sleep 1

echo "1. Navigation"
echo "-------------"
run_test "navigate to data URL" "$CLI navigate 'data:text/html,<h1>Test</h1>'"
run_test "reload page" "$CLI reload"
run_test "reload --hard" "$CLI reload --hard"

echo ""
echo "2. Page Management"
echo "------------------"
run_test "new-page" "$CLI new-page 'data:text/html,<h2>Page2</h2>'"
run_test "pages list" "$CLI pages"
run_test "select-page 0" "$CLI select-page 0"

echo ""
echo "3. Input Commands"
echo "-----------------"
$CLI navigate "data:text/html,<input id='i' style='width:200px;height:30px'/><button id='b' style='width:100px;height:30px'>Click</button>" >/dev/null 2>&1
sleep 0.5
run_test "fill input" "$CLI fill '#i' 'test'"
run_test "click button" "$CLI click '#b'"
run_test "hover element" "$CLI hover '#b'"
run_test "press key" "$CLI press Enter"

echo ""
echo "4. JavaScript Eval"
echo "------------------"
run_test "eval expression" "$CLI eval '1+1'"
run_test "eval DOM query" "$CLI eval 'document.title'"

echo ""
echo "5. Wait Commands"
echo "----------------"
$CLI navigate "data:text/html,<h1 id='h'>Title</h1>" >/dev/null 2>&1
run_test "wait selector" "$CLI wait selector --selector '#h' --timeout 3000"
run_test "wait stable" "$CLI wait stable --timeout 3000"

echo ""
echo "6. Screenshot"
echo "-------------"
run_test "screenshot png" "$CLI screenshot -o /tmp/test_cli.png"
run_test "screenshot jpeg" "$CLI screenshot -o /tmp/test_cli.jpg --format jpeg"
[ -f /tmp/test_cli.png ] && rm /tmp/test_cli.png
[ -f /tmp/test_cli.jpg ] && rm /tmp/test_cli.jpg

echo ""
echo "7. Session"
echo "----------"
run_test "session-info" "$CLI session-info"

echo ""
echo "8. Config & Devices"
echo "-------------------"
run_test "config show" "$CLI config show"
run_test "devices list" "$CLI devices"
run_test "emulate iPhone 14" "$CLI emulate 'iPhone 14'"

echo ""
echo "9. JSON Output"
echo "--------------"
check_json "session-info --json" "$CLI session-info --json"
check_json "pages --json" "$CLI pages --json"
check_json "devices --json" "$CLI devices --json"

echo ""
echo "10. Error Handling"
echo "------------------"
run_test "invalid selector fails" "$CLI click '#nonexistent'" 1
run_test "invalid page index fails" "$CLI select-page 999" 1

echo ""
echo "11. Stop Session"
echo "----------------"
run_test "stop" "$CLI stop"

echo ""
echo "================================================"
echo "Results: $(green "$PASS passed"), $(red "$FAIL failed")"
echo "================================================"

[ "$FAIL" -eq 0 ]
