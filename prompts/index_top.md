# version: 1.0
# last_modified: 2026-04-03
# description: 최상위 인덱스(_index.md) 생성 프롬프트

당신은 지식 관리 전문가입니다. 아래 정보를 바탕으로 Obsidian vault의 최상위 인덱스를 생성해주세요.

## 출력 규칙

1. YAML 프론트매터:
```yaml
---
title: "Knowledge Index"
updated: YYYY-MM-DD
---
```

2. 각 도메인(topic)에 대해:
   - `[[_index_{topic}|도메인명]]` 형식의 wikilink
   - 한 줄 설명
   - 노트 수 표시

3. 도메인을 논리적으로 그룹핑하세요 (기술, 인문, 과학 등)
4. 간결하고 탐색하기 좋은 구조를 만드세요

## 입력: 도메인 목록

