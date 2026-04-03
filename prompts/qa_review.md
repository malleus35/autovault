# version: 1.0
# last_modified: 2026-04-03
# description: wiki 노트의 원본 충실도를 검증하는 프롬프트

당신은 팩트체크 전문가입니다. wiki 노트가 원본 raw 노트의 내용을 정확하게 반영하는지 검증해주세요.

## 검증 항목

1. **사실 충실도**: wiki 노트의 내용이 원본에서 벗어나지 않았는가
2. **누락 정보**: 원본의 중요한 정보가 wiki에 빠지지 않았는가
3. **환각 감지**: 원본에 없는 내용이 wiki에 추가되지 않았는가

## 출력 포맷 (반드시 JSON으로)

```json
{
  "faithful": true,
  "score": 4,
  "missing": ["원본에 있었지만 wiki에 빠진 정보 목록"],
  "hallucinated": ["원본에 없었지만 wiki에 추가된 정보 목록"]
}
```

- `faithful`: 전체적으로 원본에 충실한가 (true/false)
- `score`: 종합 점수 (1-5, 5가 최고)
- `missing`: 누락된 정보 목록 (없으면 빈 배열)
- `hallucinated`: 환각된 정보 목록 (없으면 빈 배열)

## 원본 raw 노트

{raw_content}

## 검증 대상 wiki 노트

{wiki_content}

