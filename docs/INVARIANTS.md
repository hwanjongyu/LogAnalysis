# LogAnalysis System Invariants & Regression Prevention Guide

> **목적**: 기능 추가나 버그 수정 과정에서 기존에 이미 완성된 필수 기능이 의도치 않게 삭제되거나 손상되는 **기능 회귀(Silent Regression)**를 원천 방지하기 위한 핵심 아키텍처 불변 규칙(Invariants) 명세서입니다.

---

## 1. 파일 다이얼로그 (Native File Dialogs)

### `[INV-01-A]` 확장자 필터 유지 원칙
- **로그 파일 열기 다이얼로그 (`prompt_open_file`)**:
  - `Log Files (*.log, *.txt)` (`&["log", "txt"]`)
  - `All Files (*.*)` (`&["*"]`)
- **필터 프로필 열기/저장 다이얼로그 (`prompt_load_filters`, `prompt_save_filters`)**:
  - `JSON Profile (*.json)` (`&["json"]`)
  - `All Files (*.*)` (`&["*"]`)
- **금지 사항**: GPUI 내장 `cx.prompt_for_paths`는 파일 확장자 필터 드롭다운을 지원하지 않으므로, 필터링이 필요한 파일 대화창에 단독으로 사용해서는 안 됩니다.

### `[INV-01-B]` 모달 창 계층 및 X11 패닉 방어
- GPUI 0.2.2의 `X11Window::window_handle()`은 업스트림에서 `unimplemented!()`로 작성되어 있어 직접 호출 시 즉시 크래시(Panic)가 발생합니다.
- 따라서 X11 환경에서는 `w.window_handle()`을 직접 호출하지 않고, `CustomX11WindowHandle`과 `get_active_x11_window_id()`를 통해 안전하게 윈도우 매니저의 포커스를 바인딩해야 합니다.
- 다이얼로그는 항상 부모 창인 `LogAnalysis`의 상단(앞쪽)에 모달 형태로 표시되어야 합니다.

---

## 2. 동시성 및 비동기 작업 취소 (Concurrency & Cancellation)

### `[INV-02-A]` 토큰 분리 원칙
- 필터링 취소 토큰(`filter_cancel_gen: Arc<AtomicU64>`)과 검색 취소 토큰(`search_cancel_gen: Arc<AtomicU64>`)은 **반드시 상호 독립적**으로 관리되어야 합니다.
- 검색창(Find Bar)에서 텍스트를 입력하거나 닫는 행위가 대용량 로그 필터링 계산을 취소시켜서는 안 됩니다.

### `[INV-02-B]` UI 메인 스레드 I/O 차단 금지
- 필터 프로필 JSON 저장/불러오기, 대용량 로그 파일 읽기 작업은 반드시 비동기 백그라운드 태스크(또는 `background_executor`)에서 수행하여 UI 렌더링 스레드가 멈추지 않아야 합니다.

---

## 3. 스트리밍 및 메모리 안전성 (Streaming & Memory Safety)

### `[INV-03-A]` ADB 스트리밍 한계선 (Bounded Reader)
- `MAX_STREAM_LINE_BYTES` (64KB): 개행 문자가 누락된 무한 스트림으로부터 메모리 고갈(OOM)을 방지하기 위해 스트리밍 라인 리더는 반드시 바이트 상한선을 유지해야 합니다.

### `[INV-03-B]` 링 버퍼 트림 히스테리시스 (Hysteresis Trimming)
- 실시간 스트리밍 버퍼가 최대 용량(`100,000` 줄)에 도달하면 최소 10% 단위(`90,000` 줄까지)로 일괄 드레인하여 매 라인마다 버퍼를 재할당하는 성능 저하를 방지해야 합니다.

---

## 4. 텍스트 레이아웃 및 다국어 지원 (Layout & CJK Alignment)

### `[INV-04]` CJK / East Asian 2컬럼 폭 계산
- 로그 뷰포트에서 고정폭(Monospace) 텍스트 정렬 시 동아시아 문자(한국어, 일본어, 중국어)는 ASCII 1컬럼이 아닌 2컬럼 너비로 계산되어야 합니다 (`unicode-width` 기반).

---

## 5. CLI 및 신원 명세 (CLI Flags & Identity)

### `[INV-05]` CLI 버전 플래그
- 터미널에서 `loganalysis --version`, `-V`, `-v` 입력 시 GUI를 띄우지 않고 즉시 `LogAnalysis 0.1.0` (또는 `CARGO_PKG_VERSION`)을 stdout으로 출력 후 정상 종료(exit code 0)해야 합니다.
- GUI 실행 시 CSD 타이틀바 및 하단 상태바에 일관된 버전 뱃지(`v0.1.0`)가 표시되어야 합니다.

---

## 6. 서브프로세스 생명주기 및 자원 누수 방지 (Subprocess Lifecycle & Resource Cleanup)

### `[INV-06]` 소스 전환 및 로그 클리어 시 ADB 프로세스 종료 보장
- 파일 열기(`open_file`), 드래그 앤 드롭, 또는 로그 클리어(`clear_logs`)를 실행할 때, 현재 ADB 스트리밍 중(`is_streaming == true`)이라면 **반드시 새 소스를 설정하기 전에 `stop_adb_stream(cx)`을 선행 호출**해야 합니다.
- 이를 누락하면 백그라운드 `adb logcat` 서브프로세스가 고아(Orphan/Zombie) 프로세스로 남아 CPU를 계속 점유하고 후속 스트리밍 시작을 차단합니다.

### `[INV-07]` ADB 시리얼 보안 검증 및 타임아웃 리핑 (Security & Timeout Reaper)
- ADB 디바이스 시리얼 번호는 반드시 안전한 영숫자/특수문자 패턴(`^[a-zA-Z0-9._:-]+$`) 검증을 거친 후 명령행 인자로 전달되어야 합니다 (Command Injection 원천 방어).
- 디바이스 목록 스캔(`scan_adb_devices`) 시 ADB 서버 응답 지연으로 인한 UI 프리징을 방지하기 위해 3초 타임아웃을 적용하고, 타임아웃 초과 시 서브프로세스를 강제 회수(`kill`)해야 합니다.

---

## 7. 텍스트 입력 및 IME 안전성 (TextInput & IME Safety)

### `[INV-08-A]` UTF-16 / UTF-8 문자 경계(Char Boundary) 정렬
- 한국어, 일본어, 중국어 등 조합형 문자(IME Preedit)를 처리하는 `TextInput` 컴포넌트에서 UTF-16 코드 유닛 오프셋을 UTF-8 바이트 오프셋으로 변환할 때, 반드시 `char_indices()` 기반의 유효한 문자 경계에 정렬되어야 합니다.
- 경계가 어긋날 경우 한글 타이핑 또는 클릭 시 Rust 런타임 패닉(`byte index is not a char boundary`)이 발생합니다.

### `[INV-08-B]` IME 조합 확정(Commit) 시 엔터 키 이벤트 버블링 차단
- 한글 조합 중 <kbd>Enter</kbd> 키를 눌러 글자를 확정(Commit)할 때는 반드시 `cx.stop_propagation()`을 호출하여 키 이벤트가 상위 워크스페이스로 전파되지 않도록 해야 합니다.
- 버블링이 차단되지 않으면 한글 입력 완료 순간 의도치 않게 검색이 실행되거나 모달이 닫히는 오동작이 발생합니다.

---

## 8. 실시간 스트리밍 통계 동기화 (Streaming Hit Counter Synchronization)

### `[INV-09]` 링 버퍼 트림 시 히트 카운트 감산 동기화
- 장시간 실시간 ADB 스트리밍 중 링 버퍼 용량(100,000줄)을 초과하여 오래된 라인을 드레인(Trim)할 때, 제거된 라인들 중에 포함된 필터 매칭 라인 수만큼 `res.total_matched_lines` 및 `res.filter_counts`를 반드시 동기화 감산해야 합니다.
- 이를 누락하면 실제 메모리에 남아 있는 가시 라인 수보다 히트 카운트 수치가 무한히 누적되어 드리프트(Desync)됩니다.

---

## 9. 파일 시스템 원자성 및 접근성 (File Atomicity & Contrast)

### `[INV-10]` 필터 프로필의 원자적 저장 (Atomic File Write)
- 필터 프로필 JSON 저장 시 기존 파일에 직접 덮어쓰지 않고, 임시 파일(`*.tmp`)에 먼저 기록한 후 원자적 이름 변경(`std::fs::rename`) 방식으로 교체해야 합니다.
- 저장 도중 비정상 종료되더라도 기존 설정 파일이 깨지거나 빈 파일로 남는 데이터 유실을 방지합니다.

### `[INV-11]` 테마 상속 및 하이라이트 가독성 (Contrast & Theme Inheritance)
- 텍스트 색상만 지정된 필터 규칙은 시스템 테마 배경색을 상속받아야 하며, 배경색만 지정된 필터는 테마 전경색을 상속받아야 합니다.
- 테마 전환(<kbd>Ctrl+T</kbd>) 시 전경색과 배경색이 같아져 글자가 투명해지는 현상이 없어야 하며, WCAG AA 기준(4.5:1 이상)을 충족해야 합니다.

---

## 10. 회귀 방지 검증 체크리스트 (Verification Checklist)

코드 수정 시 반드시 다음 단계를 거쳐야 합니다:

1. **자동화 테스트 통과**: `cargo test --workspace` (49개 단위/통합 테스트 전체 통과 확인)
2. **다이얼로그 사양 검증**: 파일 열기/저장 창에 `*.log`, `*.txt`, `*.json`, `*.*` 필터가 정상 포함되어 있는지 단위 테스트(`dialogs::tests`)로 증명
3. **컴파일 및 린트**: `cargo check --workspace` 및 `cargo clippy --workspace` (경고 0건 유지)
4. **빌드 검증**: `./scripts/build-appimage.sh`로 패키징 정상 여부 확인
