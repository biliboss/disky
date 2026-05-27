---
name: disky
description: Guided disk-cleanup wizard. Triages the user's intent (free space / what grew / why slow), proposes top-3 cleanup targets via `disky top` + `cleanup --dry-run`, confirms each via AskUserQuestion, then applies `disky cleanup --apply --reversible` (Trash, restorable). Use whenever the user says "/disky", "where did my disk go", "what's eating my SSD", "scan my machine", "find cleanup", "preciso de espaço", "tá lento", or any disk-related ask.
---

# /disky — triage → propose → confirm → apply

Guides the user through a safe cleanup in 4 stages, all driven by `AskUserQuestion`. No web server, no FastHTML, no MCP — just the CLI + the wizard.

## Prerequisites

- `disky` binary on PATH (`cargo install --path . --force` from `~/src/disky`)
- Default browser configured (only for the optional final HTML report)

## Stage 1 · Triage

Open with **one** `AskUserQuestion` to classify intent. This becomes the wizard's spine.

```
question: "Disky · qual o problema?"
header:   "intent"
options:
  - label: "Liberar espaço — preciso de GBs já (Recommended)"
    description: "Roda `disky cleanup --dry-run` + `disky top`, propõe os 3 maiores hits reversíveis (node_modules, target/, __pycache__, etc)."
  - label: "Investigar — o que cresceu desde o último scan"
    description: "Roda `disky growth` entre @latest~1 e @latest. Sem propor delete — só explica."
  - label: "Diagnosticar — algo tá lento ou suspeito"
    description: "Roda `disky churn --over 7d` + `disky old --older-than 1y`. Aponta dirs que mudam toda hora ou stale há muito."
```

User answer routes to the matching stage 2 flow. **Persist the choice** via a memory note so future invocations can default.

## Stage 2 · Scan + propose

### Path A · Free space (most common)

```bash
DB=/tmp/disky-skill-$(date +%s).db
disky scan ${1:-$HOME} --db $DB --format json    # stream NDJSON progress to user
disky top --snapshot $DB --limit 20 --physical --format json > /tmp/disky-top.json
disky cleanup --snapshot $DB --format json > /tmp/disky-cleanup.json
```

Then build a **propose-3 table** from `disky cleanup` output. Rank by `total_bytes` per category. Show in chat:

```
| # | Target            | Hits | Recovers   | Path examples              |
|---|-------------------|------|------------|----------------------------|
| 1 | node_modules      | 18   | 4.2 GB     | ~/src/{a,b,c,...}/node_modules |
| 2 | target            |  7   | 8.1 GB     | ~/src/{disky,…}/target     |
| 3 | __pycache__       | 42   | 230 MB     | scattered                  |
```

**Surface OrbStack sparse-file caveat** if a single entry in `disky top` reports > 1 TB logical. Use `--physical` (already done above) and note: "OrbStack disk image reports logical size; real physical is much smaller — `--physical` shows truth."

### Path B · Investigate growth

```bash
disky list --format json | jq '.records[0:2]'    # @latest + previous
disky growth <prev_id> @latest --format json --limit 20
```

Output goes straight to user as a "what grew" digest. No cleanup proposed unless they ask.

### Path C · Diagnose

```bash
disky churn --snapshot @latest --over 7d --format json --limit 15
disky old   --snapshot @latest --older-than 1y --format json --limit 15
```

Surface churn outliers ("this dir mutates every minute — log? cache? watch path?") + stale-but-large ("hasn't changed in 14 months, 2.3 GB").

## Stage 3 · Confirm each target

For Path A, ask **one `AskUserQuestion` per proposed target**, in descending recovery order. Format:

```
question: "Limpar node_modules? 18 dirs, libera 4.2 GB"
header:   "target #1"
options:
  - label: "Sim — move pra Trash (Recommended, reversível)"
    description: "Roda `disky cleanup --apply --reversible --target node_modules`. Arquivos vão pra ~/.Trash/, recuperáveis via Finder."
  - label: "Sim — delete permanente"
    description: "Roda `disky cleanup --apply --target node_modules` SEM --reversible. Irreversível. Use só se Trash não couber."
  - label: "Pula — mantém"
    description: "Próximo target."
  - label: "Ver os paths antes"
    description: "Imprime os 18 paths em chat, depois pergunta de novo."
```

Loop até user cobrir todos os 3 propostos OU mandar "chega".

## Stage 4 · Apply

Para cada confirmação positiva, execute o comando exato mostrado:

```bash
disky cleanup --snapshot $DB --apply --reversible --target <name> --format json
```

Pós-apply, mostra o envelope `{applied: true, removed:[...], records:[...]}` em chat. Soma `total_bytes` recuperado ao longo do wizard.

## Defaults & safety

- **Sempre `--reversible` default**. Hard delete só se user pedir explicitamente E confirmar 2× (segundo AskUser).
- **Nunca invente um target.** Só roda cleanup pra targets que o stage 2 propôs e o stage 3 confirmou.
- **Nunca encadeie múltiplos `--apply` num único call** sem AskUser entre cada um.
- **Snapshot persiste**: scan vai pra `/tmp/disky-skill-<unix>.db` E é auto-salvo em `~/Library/Application Support/disky/` pelo binário. Não delete o /tmp DB no fim — pode servir pra próximo `/disky`.

## Optional · final HTML report

Após apply (ou ao fim do path B/C), oferta:

```
AskUserQuestion:
  "Gerar HTML report com tudo isso?"
  options:
    - "Sim — abre no browser"
    - "Não, basta o chat"
```

Se sim: `uv run claude-skill/disky/render.py /tmp/disky-skill-<unix>.db > /tmp/disky-report.html && open /tmp/disky-report.html`. Render é puramente visual — botões ficam como `<code>` blocks click-to-copy (sem POST/server).

## Tone

- **Mostra o comando exato** antes de propor click. "Vou rodar `disky cleanup --apply --reversible --target node_modules`."
- **Volume realista**: "libera 4.2 GB" não "libera centenas de mega de espaço".
- **Sem hedging**: se cleanup vai pra Trash, não pergunte "tem certeza?" três vezes — uma confirmação é suficiente quando reversível.

## Anti-patterns

- ❌ Rodar `cleanup --apply` sem AskUser explícito.
- ❌ Propor target que não saiu do `disky cleanup --dry-run`.
- ❌ Encadear delete + delete + delete sem confirmação entre.
- ❌ Inventar GBs ("liberará ~5 GB") quando o output diz `total_bytes: 2147483648`.
- ❌ Abrir HTML report ANTES do triage/propose/apply. HTML é cherry on top, não substituto do wizard.
- ❌ Usar `disky web` ou `disky-mcp` (não existem desde v0.10.0).

## Invocation hints

Match estes intents:

- "/disky"
- "scan my machine" · "scan minha máquina"
- "where did my disk go" · "cadê meu espaço"
- "what's eating my SSD" · "tá comendo meu disco"
- "disky report" · "report do disco"
- "find cleanup" · "limpa pra mim"
- "preciso de espaço" · "preciso liberar GB"
- "tá lento, vê o disco" · "investiga o disco"
- "what changed" · "o que cresceu"
- "what's churning" · "o que tá mudando demais"

Quando qualquer desses aparece, **call this skill before answering generically.**
