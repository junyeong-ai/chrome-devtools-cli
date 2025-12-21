# Chrome DevTools CLI

[![Rust](https://img.shields.io/badge/rust-1.91.1%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![DeepWiki](https://img.shields.io/badge/DeepWiki-junyeong--ai%2Fchrome--devtools--cli-blue.svg?logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAyCAYAAAAnWDnqAAAAAXNSR0IArs4c6QAAA05JREFUaEPtmUtyEzEQhtWTQyQLHNak2AB7ZnyXZMEjXMGeK/AIi+QuHrMnbChYY7MIh8g01fJoopFb0uhhEqqcbWTp06/uv1saEDv4O3n3dV60RfP947Mm9/SQc0ICFQgzfc4CYZoTPAswgSJCCUJUnAAoRHOAUOcATwbmVLWdGoH//PB8mnKqScAhsD0kYP3j/Yt5LPQe2KvcXmGvRHcDnpxfL2zOYJ1mFwrryWTz0advv1Ut4CJgf5uhDuDj5eUcAUoahrdY/56ebRWeraTjMt/00Sh3UDtjgHtQNHwcRGOC98BJEAEymycmYcWwOprTgcB6VZ5JK5TAJ+fXGLBm3FDAmn6oPPjR4rKCAoJCal2eAiQp2x0vxTPB3ALO2CRkwmDy5WohzBDwSEFKRwPbknEggCPB/imwrycgxX2NzoMCHhPkDwqYMr9tRcP5qNrMZHkVnOjRMWwLCcr8ohBVb1OMjxLwGCvjTikrsBOiA6fNyCrm8V1rP93iVPpwaE+gO0SsWmPiXB+jikdf6SizrT5qKasx5j8ABbHpFTx+vFXp9EnYQmLx02h1QTTrl6eDqxLnGjporxl3NL3agEvXdT0WmEost648sQOYAeJS9Q7bfUVoMGnjo4AZdUMQku50McDcMWcBPvr0SzbTAFDfvJqwLzgxwATnCgnp4wDl6Aa+Ax283gghmj+vj7feE2KBBRMW3FzOpLOADl0Isb5587h/U4gGvkt5v60Z1VLG8BhYjbzRwyQZemwAd6cCR5/XFWLYZRIMpX39AR0tjaGGiGzLVyhse5C9RKC6ai42ppWPKiBagOvaYk8lO7DajerabOZP46Lby5wKjw1HCRx7p9sVMOWGzb/vA1hwiWc6jm3MvQDTogQkiqIhJV0nBQBTU+3okKCFDy9WwferkHjtxib7t3xIUQtHxnIwtx4mpg26/HfwVNVDb4oI9RHmx5WGelRVlrtiw43zboCLaxv46AZeB3IlTkwouebTr1y2NjSpHz68WNFjHvupy3q8TFn3Hos2IAk4Ju5dCo8B3wP7VPr/FGaKiG+T+v+TQqIrOqMTL1VdWV1DdmcbO8KXBz6esmYWYKPwDL5b5FA1a0hwapHiom0r/cKaoqr+27/XcrS5UwSMbQAAAABJRU5ErkJggg==)](https://deepwiki.com/junyeong-ai/chrome-devtools-cli)

> **[English](README.en.md)** | **한국어**

**터미널에서 Chrome을 완전히 제어하세요.** 스크린샷부터 자동화, 성능 분석까지 — 브라우저 없이 모든 작업을 수행할 수 있습니다.

---

## 왜 Chrome DevTools CLI인가?

- **빠름** — 데몬 아키텍처로 브라우저 연결 재사용, 밀리초 단위 명령 실행
- **완전함** — 30+ 명령어로 Chrome 전체 기능 커버
- **자동화** — JSON 출력, 이벤트 캡처, Playwright 스크립트 생성

---

## 빠른 시작

```bash
# 설치
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
./scripts/install.sh

# 사용
chrome-devtools-cli navigate "https://example.com" --user-profile
chrome-devtools-cli screenshot -o page.png
chrome-devtools-cli click "#button"
```

---

## 주요 기능

### 브라우저 자동화
```bash
chrome-devtools-cli navigate "https://example.com"    # 페이지 이동
chrome-devtools-cli click "#login"                    # 요소 클릭
chrome-devtools-cli fill "#email" "user@test.com"     # 입력 필드 채우기
chrome-devtools-cli type "#search" "검색어" --delay 50  # 타이핑
chrome-devtools-cli press Enter                       # 키 입력
chrome-devtools-cli select "#dropdown" --label "옵션" # 드롭다운 선택
```

### 스크린샷 & PDF
```bash
chrome-devtools-cli screenshot -o page.png                  # 뷰포트
chrome-devtools-cli screenshot -o full.png --full-page      # 전체 페이지
chrome-devtools-cli screenshot -o el.png --selector "#hero" # 특정 요소
chrome-devtools-cli pdf -o page.pdf                         # PDF 내보내기
```

### 세션 녹화 & 이벤트 조회
```bash
# 브라우저 확장도구로 녹화 시작/중지
chrome-devtools-cli history events --user-profile --last 10m
chrome-devtools-cli history recordings --user-profile
chrome-devtools-cli history export --user-profile --format playwright
```

### 성능 분석
```bash
# CLI로 직접 트레이스 캡처
chrome-devtools-cli trace "https://example.com" -o trace.ndjson

# 또는 확장도구에서 Start Trace 버튼으로 녹화 시작/중지

# 트레이스 분석 (Core Web Vitals)
chrome-devtools-cli analyze trace.ndjson
# LCP 1.8s [Good] | CLS 0.03 [Good] | TTFB 280ms [Good]
```

### 디바이스 에뮬레이션
```bash
chrome-devtools-cli emulate "iPhone 14"
chrome-devtools-cli viewport 1920 1080 --pixel-ratio 2
chrome-devtools-cli devices                           # 8개 프리셋 목록
```

### 데이터 수집
```bash
chrome-devtools-cli network --domain api.example.com  # 네트워크 요청
chrome-devtools-cli console --filter error            # 콘솔 메시지
chrome-devtools-cli eval "document.title"             # JavaScript 실행
chrome-devtools-cli cookies list                      # 쿠키 조회
```

---

## 설치

### 자동 설치 (권장)
```bash
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
./scripts/install.sh
```

### 소스 빌드
```bash
git clone https://github.com/anthropics/chrome-devtools-cli && cd chrome-devtools-cli
cargo build --release
cp target/release/chrome-devtools-cli ~/.local/bin/
```

**요구사항**: Rust 1.91.1+

---

## 설정

### 설정 파일
`~/.config/chrome-devtools-cli/config.toml`:
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
chrome-devtools-cli config init    # 기본 설정 생성
chrome-devtools-cli config show    # 현재 설정 표시
chrome-devtools-cli config edit    # 에디터로 편집
```

**우선순위**: CLI 옵션 > 환경변수 > 설정 파일

---

## 명령어 참조

| 명령어 | 설명 |
|--------|------|
| `navigate <url>` | URL 이동 |
| `screenshot` | 스크린샷 |
| `click <selector>` | 요소 클릭 |
| `fill <selector> <text>` | 입력 필드 채우기 |
| `type <selector> <text>` | 타이핑 (딜레이) |
| `press <key>` | 키 입력 |
| `select <selector>` | 드롭다운 선택 |
| `trace <url>` | 페이지 로드 중 트레이스 캡처 |
| `analyze <file>` | 트레이스 분석 (Core Web Vitals) |
| `emulate <device>` | 디바이스 에뮬레이션 |
| `eval <expr>` | JavaScript 실행 |
| `history events` | 이벤트 조회 |
| `history export` | Playwright 스크립트 생성 |

### 공통 옵션
- `--json` — JSON 출력
- `--user-profile` — 사용자 프로필 세션 유지
- `--headless=false` — 브라우저 창 표시
- `--last <duration>` — 시간 필터 (예: 10m, 2h)

---

## 서버 모드

```bash
chrome-devtools-cli server start   # 데몬 시작
chrome-devtools-cli server status  # 상태 확인
chrome-devtools-cli server stop    # 데몬 중지
```

---

## 문제 해결

### 브라우저 연결 실패
```bash
chrome-devtools-cli server stop
rm -f ~/.config/chrome-devtools-cli/session.toml
```

### Chrome 재설치
```bash
./scripts/install.sh --reinstall-chrome
```

### 디버그
```bash
RUST_LOG=debug chrome-devtools-cli navigate "https://example.com"
```

---

## 지원

- [GitHub Issues](https://github.com/anthropics/chrome-devtools-cli/issues)
- [개발자 가이드](CLAUDE.md)

---

<div align="center">

**[English](README.en.md)** | **한국어**

Made with Rust

</div>
