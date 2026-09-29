repo: mmnzns/Parla
branch: main

## Last sync
date: 2026-09-29T19:53:39Z

### Updated in this project
- Redesign exploration: 3 visual directions (Workbench, Studio, Night Shift), light + dark tokens
- Casual-user IA: 9 nav items in 3 groups, plain-language labels
- Screens: Home, History, AI cleanup, Speech model, App profiles, Settings, Onboarding, recorder pill, token/component sheet

## Screen map
| Screen | Repo files |
|---|---|
| Tokens (light/dark) | src/index.css |
| App shell + sidebar | src/App.tsx, src/components/Sidebar.tsx, src/components/CompactHero.tsx |
| Home | src/components/DashboardPanel.tsx |
| History | src/components/HistoryPanel.tsx |
| AI cleanup | src/components/EnhancementPanel.tsx, EnhancementScreenContext.tsx, PromptEditor.tsx, LlmLocalPanel.tsx |
| Speech model | src/components/ModelsPage.tsx, ModelPerformancePanel.tsx |
| App profiles | src/components/PowerModePanel.tsx |
| Settings | src/components/SettingsPanel.tsx, PostProcessingPanel.tsx, PermissionsPanel.tsx |
| Onboarding | src/components/Onboarding.tsx |
| Recorder pill | src/components/MiniRecorderView.tsx, RecorderPopoverView.tsx |
| Copy | src/i18n/locales/en.json |
