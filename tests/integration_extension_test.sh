#!/bin/bash
#
# Extension Integration Test
#
# 이 스크립트는 Extension → CLI 통신을 테스트합니다.
#
# 테스트 시나리오:
# 1. CLI로 브라우저 시작 (확장프로그램 로드)
# 2. Extension popup에서 "CLI Connected" 확인
# 3. Element selection 테스트
# 4. Recording 테스트
# 5. Screenshot 테스트
#
# 문제: Chrome Extension content script는 isolated world에서 실행되어
# CDP AddBindingParams로 주입된 __cdtcli__ 함수(main world)에 접근 불가
#
# 해결:
# 1. sendToCli(): <script> 태그를 동적으로 생성하여 main world에서 실행
# 2. checkCliConnection(): world: 'MAIN' 옵션 추가

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CLI="$SCRIPT_DIR/target/release/chrome-devtools-cli"
EXTENSION_DIR="$SCRIPT_DIR/extension/dist"

echo "=== Chrome DevTools CLI - Extension Integration Test ==="
echo ""
echo "Extension path: $EXTENSION_DIR"
echo ""

# Check if extension is built
if [ ! -f "$EXTENSION_DIR/manifest.json" ]; then
    echo "Building extension..."
    cd "$SCRIPT_DIR/extension" && npm run build
fi

# Check if CLI is built
if [ ! -f "$CLI" ]; then
    echo "Building CLI..."
    cd "$SCRIPT_DIR" && cargo build --release
fi

echo "=== Test Procedure ==="
echo ""
echo "1. 브라우저 시작 (CLI):"
echo "   $CLI navigate \"https://example.com\" --keep-alive --headless false"
echo ""
echo "2. 확장프로그램 로드 (수동):"
echo "   - chrome://extensions 열기"
echo "   - 개발자 모드 활성화"
echo "   - 'Load unpacked' 클릭"
echo "   - $EXTENSION_DIR 선택"
echo ""
echo "3. CLI 연결 확인:"
echo "   - 확장프로그램 popup 클릭"
echo "   - 'CLI Connected' 표시 확인"
echo ""
echo "4. Element Selection 테스트:"
echo "   - Popup에서 'Select Element' 클릭"
echo "   - 페이지에서 요소 선택"
echo "   - CLI 로그에서 'element_selected' 이벤트 확인"
echo ""
echo "5. Recording 테스트:"
echo "   - Popup에서 'Start Recording' 클릭"
echo "   - 페이지에서 클릭, 입력 등 행동"
echo "   - 'Stop Recording' 클릭"
echo "   - CLI sessions 명령으로 녹화 데이터 확인"
echo ""

# Ask user if they want to start the test
read -p "테스트를 시작하시겠습니까? (y/n): " choice
if [ "$choice" != "y" ] && [ "$choice" != "Y" ]; then
    echo "테스트 취소됨"
    exit 0
fi

echo ""
echo "=== Starting Browser ==="

# Start browser with keep-alive
$CLI navigate "https://example.com" --keep-alive --headless false &
CLI_PID=$!

sleep 3

echo ""
echo "브라우저가 시작되었습니다."
echo ""
echo "다음 단계:"
echo "1. chrome://extensions에서 확장프로그램 로드"
echo "2. example.com 탭으로 이동"
echo "3. 확장프로그램 아이콘 클릭하여 popup 열기"
echo "4. 'CLI Connected' 상태 확인"
echo ""
echo "테스트 완료 후 Enter를 누르세요..."
read

echo ""
echo "=== Session Info ==="
$CLI session-info

echo ""
echo "=== Checking Extension Events ==="

# Get session ID
SESSION_ID=$($CLI session-info --json 2>/dev/null | jq -r '.session_id // empty' || echo "")

if [ -n "$SESSION_ID" ]; then
    echo "Session ID: $SESSION_ID"

    # Check for extension events in session storage
    SESSION_DIR="$HOME/.config/chrome-devtools-cli/sessions/$SESSION_ID"

    if [ -f "$SESSION_DIR/extension.ndjson" ]; then
        echo ""
        echo "=== Extension Events ==="
        cat "$SESSION_DIR/extension.ndjson" | head -20
    else
        echo "No extension events recorded yet."
        echo "(확장프로그램에서 Element Select나 Recording을 실행하세요)"
    fi
fi

echo ""
echo "=== Test Complete ==="
echo ""
echo "브라우저를 종료하려면:"
echo "  $CLI stop"
echo ""
echo "세션 데이터 확인:"
echo "  $CLI sessions list"
echo "  $CLI sessions show <session_id>"
