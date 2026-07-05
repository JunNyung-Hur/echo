//! Chat-agent tool specs + stage/capability gating.
//!
//! Meetzy d75150c `tool_specs.py` 1:1 이식 (도메인 워딩만 회의록→노트).
//! 차이점(승인된 이탈): ① `set_theme` — echo 는 테마 프리셋 선택제라 디자인 요청을
//! 거절하지 않고 이 도구로 전환한다. ② `get_recording_download_url` 은 로컬 앱이라
//! URL 대신 파일 버튼(로컬 경로)을 렌더한다. ③ `write_note` 는 echo 고유
//! freeform 경로용으로 유지.

use serde_json::{json, Value};

/// Stage → 노출 도구. ask_user 는 모든 단계에서 노출 — 질문은 단계와 무관한
/// 행동이고, 도구로 물화해야 루프가 질문 시점에 하드스톱할 수 있다(자문자답
/// 구조적 차단). 메타데이터 변경 도구는 없음(제목=본문 `# ` 헤딩에서 파생).
fn stage_tools(stage: &str) -> Vec<&'static str> {
    match stage {
        // 노트 필기형 (echo 고유) — 받아적기/전체 정돈은 write_note, 국소 수정·삭제·
        // 정정은 read→edit_minutes(str_replace), 디자인은 set_theme.
        "freeform" => vec![
            "write_note",
            "read_minutes",
            "edit_minutes",
            "set_theme",
            "ask_user",
        ],
        "before" | "recording" => vec!["ask_user"],
        "transcribing" => vec![
            "get_recording_download_url",
            "retry_transcribe",
            "retry_failed_task",
            "ask_user",
        ],
        // `done` (and any unknown stage → full set). set_theme 는 freeform 전용 —
        // 회의록 작성형은 고정 기본 테마(Meetzy 동일: 디자인 요청은 안내로 거절).
        _ => vec![
            "read_minutes",
            "edit_minutes",
            "get_recording_download_url",
            "retry_transcribe",
            "retry_failed_task",
            "read_transcript",
            "ask_user",
        ],
    }
}

/// Tools the LLM should see this turn. Stage gating + capability gating: any
/// tool whose UI action is `hidden` (in `hidden_tools`) is stripped so the LLM
/// can't trigger an action the user can't see. (ports tools_for)
pub fn tools_for(stage: &str, hidden_tools: &[String]) -> Vec<Value> {
    let allowed = stage_tools(stage);
    all_specs()
        .into_iter()
        .filter(|spec| {
            let name = spec["function"]["name"].as_str().unwrap_or("");
            allowed.contains(&name) && !hidden_tools.iter().any(|h| h == name)
        })
        .collect()
}

pub fn all_specs() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "read_minutes",
                "description": "현재 활성 노트의 *최신* 본문(마크다운 전체)을 조회합니다. **노트를 편집(edit_minutes)하기 직전, 또는 내용 질문에 답하기 전에 이 도구로 현재 본문을 먼저 확인하세요.** edit_minutes 의 old 는 *여기서 조회한 현재 본문에 있는 그대로* 복사해야 정확히 매칭됩니다. (직전에 편집했다면 본문이 이미 바뀌었으니, 과거 대화의 옛 문구가 아니라 *지금 조회한 본문*을 기준으로 하세요.)",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "edit_minutes",
                "description": "노트 *내용을 바꾸는 모든 편집*을 str_replace로 적용합니다(done 단계). 오탈자·오단어 교정, 문장/표현 교정, 용어 정정, 특정 구절 수정, 문구·주석 추가, 그리고 **압축·요약·확장·내용 정리(아무리 방대해도)** 전부 이걸로. **먼저 `read_minutes`로 현재 본문을 확인**하고, 거기서 *바꿀 부분을 그대로 찾아* old/new 로 지정. old는 바꿀 지점을 **유일하게 식별할 *최소한*의 길이**로 잡는다 — 유일하게 매칭되는 한 짧을수록 좋다(블록·섹션 전체를 통째로 복사하면 인자가 비대해져 적용이 수십 초씩 느려짐). 짧아서 여러 곳에 걸려 모호할 때만 그만큼만 맥락을 늘린다. **추가/삽입은 붙일 위치 *바로 옆 한 줄(또는 짧은 고유 구절)만* old 로 잡고 new=그 줄+새 내용**. 앞뒤 여러 줄이나 블록 전체를 old/new 에 복사하지 말 것. **이동(어떤 부분을 다른 위치로 옮기기)은 반드시 *한 번의 edits 배열에 빼기+넣기 두 연산을 함께* 넣는다**: 옛 위치를 없애는 edit(old=옛위치 포함 스니펫→new=그 부분 뺀 것)과 새 위치에 넣는 edit(old=새위치 스니펫→new=거기+옮길 내용)를 같이. 삭제만 하고 다시 안 넣으면 내용이 사라진다. 여러 곳이면 edits 배열에 여러 개. **'X 문구를 빼줘/삭제해줘'는 X라는 *문구만* 제거하는 것이다 — X가 들어 있는 줄·항목 전체를 지우면 안 된다.** new는 old에서 X만 뺀 나머지로, 같은 줄의 다른 내용은 전부 보존한다. 예: old=`- 배경 설명: 핵심 내용` 에서 '배경 설명:' 문구를 빼달라면 new=`- 핵심 내용` (줄 유지, 문구만 제거). 줄·항목 자체를 지우는 건 사용자가 '그 줄/항목을 지워달라'고 명시했을 때만이다. **정정('B가 아니라 A야' / 'A야, B가 아니라' / 'A가 맞아')의 방향: old는 *반드시 read_minutes로 확인한 본문에 실제로 있는* 잘못된 표기(B)이고, new는 사용자가 맞다고 한 표기(A)다.** 발화에 나온 순서로 old/new를 정하지 마라 — 사용자가 맞는 값을 먼저 말했을 수 있다. old로 쓰려는 문자열이 read 결과에 그대로 존재하는지 항상 확인하고, 본문에 없는 문자열을 old로 지어내지 마라.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "edits": {
                            "type": "array",
                            "description": "적용할 편집들. 각 항목은 old를 new로 치환.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "old": { "type": "string", "description": "현재 본문에 *있는 그대로*의 스니펫. replace_all=false면 본문에서 유일하게 식별되도록 주변 맥락 포함." },
                                    "new": { "type": "string", "description": "그 자리에 들어갈 새 텍스트." },
                                    "replace_all": { "type": "boolean", "description": "old의 *모든 출현*을 바꾸려면 true(기본 false=유일 매칭). 같은 표기/용어를 문서 전체에서 바꾸는 요청은 보통 *전부*가 의도이므로 true(예: 어떤 용어를 다른 표기로 통일). 특정 한 곳만 바꿀 때만 false로 두고 맥락으로 유일하게." }
                                },
                                "required": ["old", "new"]
                            }
                        },
                        "user_request": {
                            "type": "string",
                            "description": "사용자의 원문 요청(말투만 정리). 무엇을 왜 고치는지 기록·설명용."
                        }
                    },
                    "required": ["edits"]
                }
            }
        }),
        // echo 고유 — 디자인은 본문에서 분리된 테마 프리셋. 디자인/스타일 요청은
        // 거절하지 않고 이 도구로 전환한다(승인된 Meetzy 이탈점).
        json!({
            "type": "function",
            "function": {
                "name": "set_theme",
                "description": "노트의 *디자인 테마*를 프리셋으로 바꿉니다. 본문 내용은 그대로 두고 시각 스타일(색·글꼴·배경·괘선)만 전환합니다. 사용자가 디자인/색/스타일/테마/분위기 변경을 요청하면 이 도구로 처리하세요 — 내용·구조·길이 변경은 edit_minutes. 프리셋: `default`(미니멀 화이트), `notepad`(노란 괘선 공책), `report`(정갈한 보고서, 네이비), `colorful`(밝고 컬러풀). 요청이 어느 프리셋인지 명확하면 바로 호출하고, 모호하면('예쁘게', '느낌 바꿔줘') ask_user 로 프리셋 선택지를 제시해 고르게 하세요.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "theme": {
                            "type": "string",
                            "enum": ["default", "notepad", "report", "colorful"],
                            "description": "적용할 테마 프리셋 id."
                        }
                    },
                    "required": ["theme"]
                }
            }
        }),
        // echo 고유 — freeform 노트 필기형 (기존 유지).
        json!({
            "type": "function",
            "function": {
                "name": "write_note",
                "description": "노트 필기형에서, 사용자가 채팅으로 전달한 *새 내용*을 노트 본문에 받아적거나(append), 이미 적힌 내용 전체의 문체 정돈(tidy)·구조 재구성(restructure)을 수행합니다. 본문이 없으면 새로 작성합니다. **국소 수정·문구 삭제·정정('X 빼줘', 'A를 B로', '~ 틀렸어')에는 쓰지 마세요 — 그건 read_minutes로 본문을 확인한 뒤 edit_minutes로 처리합니다.** 사용자 발화를 거의 그대로 user_request에 담아 전달하세요. 1-2분간 동기로 대기 후 새 노트 본문을 반환합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "user_request": { "type": "string", "description": "노트에 반영할 사용자 콘텐츠 원문만(말투만 정리). agent 자신의 약속·확인·안내 문구('앞으로 ~하겠습니다','수정할까요' 등)는 절대 넣지 말 것." },
                        "intent": { "type": "string", "enum": ["append", "tidy", "restructure"], "description": "사용자 의도. **append**=새 내용을 받아적기(기본). **tidy**=이미 적힌 내용의 말투·문체만 다듬기 — '정리해줘/다듬어줘'인데 구조를 바꾸라는 말은 없을 때(소제목·불릿·제목 손대지 않음). **restructure**=구조화를 명시적으로 요청할 때만 — '항목으로 묶어줘/소제목 달아줘/구조 잡아줘/개요 만들어줘'처럼. 사용자가 구조·스타일 변경을 명시하지 않았으면 restructure를 쓰지 말고 tidy(또는 append)로." }
                    },
                    "required": ["user_request"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_recording_download_url",
                "description": "녹음 파일을 사용자에게 전달합니다(채팅에 파일 버튼으로 표시됨). 사용자가 *명시적으로 파일/다운로드를 달라고* 했을 때만 호출하세요 ('파일 줘', '녹음 받게 해줘' 등). 단순히 '다운로드 돼?' / '녹음 있어?'처럼 *가능 여부를 묻는 질문*에는 호출하지 말고 안내만 하세요. 녹음 파일이 없거나 보존 정책으로 삭제된 경우 결과에 오류 사유가 담겨 옵니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "retry_transcribe",
                "description": "전사를 처음부터 다시 시도합니다. **기존 전사록과 노트(편집 내역 포함)가 모두 폐기**되고 재생성됩니다. **호출 전 반드시 ask_user 로 폐기 사실을 확인받아야 합니다**: 사용자가 재전사를 요청하면 즉시 호출하지 말고, 먼저 ask_user(question='기존 전사록과 노트가 모두 폐기되고 새로 생성됩니다. 다시 전사할까요?', options 비움)로 묻고 멈추세요. 사용자가 '네'라고 확인한 *다음 턴*에만 호출합니다. (직전 턴에 이 확인을 이미 받았다면 바로 호출.) 이 도구는 작업을 시작만 시키고 즉시 반환합니다. 실제 완료까지 5-10분 정도 걸린다고 사용자에게 안내하세요.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "retry_failed_task",
                "description": "현재 실패한 작업(전사 또는 노트 생성)을 자동으로 재시작합니다. **호출 조건이 매우 엄격함**: 사용자가 *명시적으로* 재시도를 지시한 발화일 때만 호출. 예: '다시 시도해줘' / '재시도해줘' / '다시 해줘' / (이전 turn에서 'X를 다시 시도해드릴까요?'라고 물은 직후) '응' / '그래' / '해줘'. **호출 금지 케이스**: 사용자가 단순 상태 질문('뭐야?' / '잘 됐어?' / '어떻게 됐어?' / '끝났어?' 등)을 하거나 화면 안내만 요청한 경우. 이런 발화는 사용자가 *상황을 인지하고 싶은 단계*지 *동작을 시키는 단계*가 아님. 답으로 사실 안내 + '다시 시도해드릴까요?' 제안까지만 하고 도구는 호출하지 말 것. 외부 AI 서버 일시 장애는 즉시 재시도로 회복 안 될 수 있어서 동의 없는 자동 재시도는 무의미한 fail 누적 → UX 악화. 실패한 노트 생성이 있으면 노트만 재생성(전사록은 그대로, 1-2분 소요), 실패한 전사가 있으면 전사부터 재시작(기존 전사록·노트 폐기, 5-10분 소요). 결과의 `retried` 필드(`minutes` 또는 `transcript`)와 `eta_minutes`로 사용자에게 정확한 소요 시간을 안내하세요. 재시작할 작업이 없으면 결과에 오류 사유가 담겨 옵니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        // ATB(AgentToolbox) authoring 에이전트의 ASK_USER_TOOL 과 동일 계약 — 질문을
        // 도구로 물화하고, 루프는 이 호출 시점에 턴을 하드스톱해 사용자에게 넘긴다.
        json!({
            "type": "function",
            "function": {
                "name": "ask_user",
                "description": "사용자 판단·선택이 꼭 필요할 때 질문하고 턴을 멈춰 사용자에게 넘긴다. **명확하면 묻지 말고 그냥 실행하라**(예: 편집 요청이 명확하면 바로 edit_minutes). 선택이 진짜 애매할 때만, 같은 질문을 여러 번 하지 말고 **이 도구를 한 번** 불러 묻고 멈춰라.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "사용자에게 할 질문 **한두 문장만**(짧고 구체적으로). **선택지 내용이나 번호 목록을 question에 넣지 마라** — 선택지는 options 필드에만 담는다(카드가 버튼으로 보여줌)."
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "고를 선택지 **2~4개**. **'네/아니오'로 답할 수 있는 질문일 때만 비워라**(네/아니오 버튼이 자동 생성 — '어떻게 할까요?' 같은 열린 질문에 비우면 질문과 버튼이 안 맞는다). 열린 질문은 반드시 선택지를 제시하고, 선택지를 만들 수 없으면 ask_user 대신 자연어로 묻고 멈춰라. 선택지가 1개뿐이면 부르지 말고 그냥 실행하라. '직접 입력' 항목은 넣지 마라(자동 추가)."
                        }
                    },
                    "required": ["question"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_transcript",
                "description": "전사록(녹음의 STT 결과 원문)을 가져와 사용자에게 보여주거나 깊은 Q&A에 활용합니다. **호출 조건이 매우 엄격함**: 사용자가 *명시적으로* 전사록 원문을 보거나 인용해달라고 한 경우에만 호출. 예: '전사록 보여줘' / '전사 읽어줘' / '전사록 그대로 좀 볼래' / '녹음에서 X 부분 원문 어떻게 말했어?'. **호출 금지 케이스 (매우 중요)**: (1) 사용자가 노트 내용에 대해 묻거나 정정·다듬기를 요청한 경우 — `read_minutes`로 현재 본문을 보고 답하거나 `edit_minutes`로 처리. (2) 사용자가 노트만으로 충분히 답할 수 있는 일반 질문 ('결정사항 뭐였어?', '참석자 누구야?')을 한 경우. (3) **사용자가 명시적으로 묻기 전에 자발적으로 *제안*하지 말 것.** '전사록에서 확인해드릴까요?' / '전사 원문을 보여드릴까요?' 류의 선제 제안은 금지. 사용자가 자기 입으로 '전사 / 원문 / 받아쓰기' 같은 단어로 요청하지 않는 한 호출하지 않음. **응답 처리**: 전사 미리보기 블록은 채팅에 자동 렌더됩니다. 도구 결과가 나온 뒤 짧은 안내 한 줄(예: '전사 원문이에요.')만 하세요. 도구 결과를 다시 그대로 출력하지 마세요 — 백엔드가 중복 없이 처리합니다.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}
