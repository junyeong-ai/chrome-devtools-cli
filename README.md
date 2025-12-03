# Chrome DevTools CLI

[![Rust](https://img.shields.io/badge/rust-1.91.1%2B%20(2024%20edition)-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.1.0-blue?style=flat-square)](https://github.com/user/chrome-devtools-cli/releases)

> **🌐 한국어** | **[English](README.en.md)**

---

> **⚡ 터미널에서 Chrome을 제어하는 브라우저 자동화 CLI**
>
> - 📸 **스크린샷** (전체 페이지, 요소 선택, PNG/JPEG/WebP)
> - 📊 **성능 분석** (Core Web Vitals: LCP, FID, CLS, TTFB)
> - 🖱️ **입력 자동화** (클릭, 타이핑, 폼 입력, 다이얼로그)
> - 🔄 **세션 유지** (명령어 간 브라우저 연결 재사용)

---

## ⚡ 빠른 시작 (1분)

```bash
# 1. 설치
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
./scripts/install.sh

# 2. 사용 시작! 🎉
chrome-devtools-cli navigate "https://example.com"
chrome-devtools-cli screenshot page.png
chrome-devtools-cli click "#button"
```

**Tip**: `--keep-alive` 플래그로 브라우저를 재사용하면 연속 작업이 빨라집니다.

---

## 🎯 주요 기능

### 스크린샷 & 녹화
```bash
# 스크린샷
chrome-devtools-cli screenshot page.png                    # 뷰포트
chrome-devtools-cli screenshot full.png --full-page        # 전체 페이지
chrome-devtools-cli screenshot el.png --selector "#hero"   # 특정 요소

# 녹화 & 내보내기
chrome-devtools-cli record -o video.mp4 --duration 10      # 화면 녹화
chrome-devtools-cli pdf -o page.pdf                        # PDF 내보내기
```

### 브라우저 자동화
```bash
# 네비게이션
chrome-devtools-cli navigate "https://example.com" --wait-for load
chrome-devtools-cli reload --hard
chrome-devtools-cli back && chrome-devtools-cli forward

# 입력
chrome-devtools-cli click "#login-button"
chrome-devtools-cli fill "#email" "user@example.com"
chrome-devtools-cli type "#search" "검색어" --delay 50
chrome-devtools-cli press Enter
chrome-devtools-cli select "#dropdown" --label "옵션 1"

# 다이얼로그 처리
chrome-devtools-cli dialog --accept --text "입력값"
```

### 성능 분석
```bash
chrome-devtools-cli trace "https://example.com" -o trace.json
chrome-devtools-cli analyze trace.json
# 출력: LCP 1.8s [Good] | FID 45ms [Good] | CLS 0.03 [Good] | TTFB 280ms [Good]
```

### 디바이스 에뮬레이션
```bash
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli viewport 1920 1080 --pixel-ratio 2
chrome-devtools-cli devices  # 8개 프리셋 목록
```

### 세션 관리
```bash
# 브라우저 재사용
chrome-devtools-cli --keep-alive navigate "https://example.com"
chrome-devtools-cli --keep-alive screenshot page.png
chrome-devtools-cli stop

# 다중 탭
chrome-devtools-cli new-page --url "https://google.com"
chrome-devtools-cli pages
chrome-devtools-cli select-page 1
chrome-devtools-cli close-page 0
```

### DOM & 접근성 검사
```bash
chrome-devtools-cli inspect "#element" --all           # 요소 상세 정보
chrome-devtools-cli query "button" --count             # 셀렉터 매칭 개수
chrome-devtools-cli a11y --interactable                # 접근성 트리
chrome-devtools-cli dom "#container" --depth 3         # DOM 트리
chrome-devtools-cli html --selector "#content"         # HTML 추출
```

### 데이터 수집 & 디버깅
```bash
chrome-devtools-cli network --domain api.example.com   # 네트워크 요청
chrome-devtools-cli console --filter error             # 콘솔 메시지
chrome-devtools-cli eval "document.title"              # JavaScript 실행
chrome-devtools-cli cookies list                       # 쿠키 조회
chrome-devtools-cli storage get "token"                # localStorage
```

### 세션 데이터 활용
```bash
chrome-devtools-cli sessions list                              # 세션 목록
chrome-devtools-cli sessions network <id> --status 500         # 에러 요청
chrome-devtools-cli sessions console <id> --level error        # 에러 로그
chrome-devtools-cli sessions export <id> --format playwright   # 스크립트 변환
```

---

## 📦 설치

### 방법 1: 설치 스크립트 (권장) ⭐

```bash
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
./scripts/install.sh
```

설치 스크립트가 자동으로:
- Rust 빌드 및 바이너리 설치 (`~/.local/bin/`)
- Chrome for Testing 다운로드
- 기본 설정 파일 생성

### 방법 2: 수동 빌드

```bash
git clone https://github.com/user/chrome-devtools-cli
cd chrome-devtools-cli
cargo build --release
cp target/release/chrome-devtools-cli ~/.local/bin/
```

**Requirements**: Rust 1.91.1+, curl, unzip

---

## ⚙️ 설정

### 설정 파일

**위치**: `~/.config/chrome-devtools-cli/config.toml`

```toml
[browser]
headless = true
port = 9222

[performance]
navigation_timeout_seconds = 30

[output]
default_screenshot_format = "png"
screenshot_quality = 90
```

### 설정 명령어

```bash
chrome-devtools-cli config init   # 기본 설정 생성
chrome-devtools-cli config show   # 현재 설정 표시
chrome-devtools-cli config edit   # 에디터로 편집
chrome-devtools-cli config path   # 설정 파일 경로
```

### 설정 우선순위

```
CLI 플래그 > 환경 변수 > 설정 파일 > 기본값
```

---

## 📚 명령어 참조

| 명령어 | 설명 | 예제 |
|--------|------|------|
| `navigate <url>` | URL 이동 | `chrome-devtools-cli navigate "https://example.com"` |
| `screenshot` | 스크린샷 | `chrome-devtools-cli screenshot -o page.png --full-page` |
| `click <selector>` | 요소 클릭 | `chrome-devtools-cli click "#button"` |
| `fill <selector> <text>` | 입력 필드 채우기 | `chrome-devtools-cli fill "#email" "user@test.com"` |
| `type <selector> <text>` | 타이핑 (딜레이) | `chrome-devtools-cli type "#input" "hello" --delay 50` |
| `press <key>` | 키 입력 | `chrome-devtools-cli press Enter` |
| `trace <url>` | 성능 트레이스 | `chrome-devtools-cli trace "https://example.com" -o trace.json` |
| `analyze <file>` | 트레이스 분석 | `chrome-devtools-cli analyze trace.json` |
| `emulate <device>` | 디바이스 에뮬레이션 | `chrome-devtools-cli emulate "iPhone 14"` |
| `eval <expr>` | JavaScript 실행 | `chrome-devtools-cli eval "document.title"` |
| `wait <condition>` | 조건 대기 | `chrome-devtools-cli wait selector --selector "#el"` |

### 공통 옵션

| 옵션 | 설명 | 적용 범위 |
|------|------|-----------|
| `--json` | JSON 형식 출력 | 모든 명령어 |
| `--keep-alive` | 브라우저 세션 유지 | 모든 명령어 |
| `--headless=false` | 브라우저 창 표시 | 모든 명령어 |
| `--port <PORT>` | 디버깅 포트 지정 | 모든 명령어 |
| `--user-profile` | 사용자 프로필 유지 | 모든 명령어 |

---

## 🔧 문제 해결

### 브라우저 연결 실패

```bash
chrome-devtools-cli stop
rm -f ~/.config/chrome-devtools-cli/session.toml
```

### 요소를 찾을 수 없음

```bash
# 페이지 로드 대기
chrome-devtools-cli navigate "https://example.com" --wait-for load

# 요소 대기
chrome-devtools-cli wait selector --selector "#element" --timeout 10000
```

### Chrome for Testing 재설치

```bash
rm -rf ~/.config/chrome-devtools-cli/chrome-for-testing
./scripts/install.sh
```

---

## 🚀 개발자 가이드

**아키텍처, 디버깅, 기여 방법**: [CLAUDE.md](CLAUDE.md) 참고

---

## 💬 지원

- **GitHub Issues**: [문제 신고](https://github.com/user/chrome-devtools-cli/issues)
- **개발자 문서**: [CLAUDE.md](CLAUDE.md)

---

<div align="center">

**🌐 한국어** | **[English](README.en.md)**

**Version 0.1.0** • Rust 2024 Edition

Made with ❤️ for automation

</div>
