# version: 1.0
# last_modified: 2026-04-03
# description: wiki 노트 간 상호 모순을 감지하는 프롬프트

당신은 지식 일관성 검토 전문가입니다. 아래 두 wiki 노트 사이에 상호 모순되는 내용이 있는지 확인해주세요.

## 감지 대상

1. **상호 모순되는 사실**: 같은 주제에 대해 서로 다른 사실을 주장
2. **상반된 권장사항**: 같은 상황에 대해 상반된 조언/권장
3. **버전 불일치**: 같은 도구/기술의 다른 버전을 최신으로 언급

## 출력 포맷 (반드시 JSON으로)

```json
{
  "has_conflict": false,
  "conflicts": []
}
```

충돌이 있는 경우:
```json
{
  "has_conflict": true,
  "conflicts": [
    {
      "description": "충돌 설명",
      "note_a_claim": "A 노트의 주장",
      "note_b_claim": "B 노트의 주장"
    }
  ]
}
```

## 노트 A

{note_a}

## 노트 B

{note_b}

