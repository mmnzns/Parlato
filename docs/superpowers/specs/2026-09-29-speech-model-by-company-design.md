# Speech model page, organised by company

Approved 2026-09-29.

## Problem

The Speech model page listed models by engine name (Whisper, Parakeet) and
file format (int8, F16, Q5_0), in Recommended / On this PC / Online tabs.
Casual users could not tell which model to pick.

## Design

- Keep the "in use" bar with the language picker at the top, and the
  performance section at the bottom.
- Replace the tabs with a side-by-side layout (like Power modes): a company
  list on the left, the chosen company's models on the right. The page opens
  on the company in use, or NVIDIA when nothing is set up.
- Company list, two labelled groups:
  - On this PC, free and private: NVIDIA (Parakeet), OpenAI (Whisper).
  - Online, needs an account key: one entry per service the user signs up
    with (Groq, Deepgram, ElevenLabs, ...). Groq notes "runs OpenAI's Whisper".
- Local companies show 2-3 plain picks, each with a plain title and the real
  model name in small print:
  - NVIDIA: "Best for English" (Parakeet Unified int8), "English, French,
    Spanish and more" (Parakeet v3 int8).
  - OpenAI: "Fastest" (Base), "Balanced" (Large v3 Turbo Q5_0), "Most
    accurate" (Large v3 Q5_0). When the dictation language is English, a
    pick uses the English-only version where one exists.
  - "Show all versions" reveals every other model with its technical name,
    plus (OpenAI) the import-your-own card. It opens by itself when the model
    in use is one of the hidden versions.
- Online companies show all their batch models directly, plus the cloud
  timeout setting.
- Licence credits stay on the cards.

## Scope

Frontend only. The company list and picks live in one config file
(`src/components/models/companies.ts`). Download, selection and engines are
unchanged; existing downloads and the active model keep working. The
onboarding model step uses the same page.
