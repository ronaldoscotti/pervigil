import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import "@fontsource/lora/400.css";
import "@fontsource/lora/500.css";
import "@fontsource/lora/600.css";
import "@fontsource/space-mono/400.css";
import "@fontsource/space-mono/700.css";

/** Injected at build time from package.json (see vite.config.ts). */
declare const __APP_VERSION__: string;

type SessionState = "Working" | "WaitingOnYou" | "YourTurn" | "Idle";
type Span = "4h" | "today" | "week";

interface SessionView {
  id: string;
  project: string;
  name: string;
  branch: string | null;
  state: SessionState;
  since: number;
  siblings: number;
  cost: number | null;
  focus: string;
  pinned: boolean;
}

interface FocusOutcome {
  raised: boolean;
  label: string;
  resume: string | null;
  error: string | null;
}

interface Segment {
  state: SessionState;
  from: number;
  to: number;
}

interface Snapshot {
  now: number;
  from: number;
  waiting: number;
  sessions: SessionView[];
  segments: Segment[];
  waitingShare: number;
  cost: number;
  tokens: number;
  notifications: boolean;
  dismissRead: boolean;
  hidden: string[];
  hooksInstalled: boolean;
  hookSnippet: string;
}

const TONE: Record<SessionState, string> = {
  Working: "working",
  WaitingOnYou: "waiting",
  YourTurn: "your-turn",
  Idle: "idle",
};

// Inline (Lucide-style) so there's no icon dependency.
const PIN_ICON = `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>`;
const CHECK_ICON = `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`;

type Lang = "en" | "zh" | "hi" | "es" | "ar" | "pt" | "fr" | "ru" | "ja" | "de";

const LANGS: Lang[] = ["en", "zh", "hi", "es", "ar", "pt", "fr", "ru", "ja", "de"];
const RTL: Set<Lang> = new Set(["ar"]);

const STRINGS: Record<Lang, Record<string, string>> = {
  en: {
    working: "Working",
    waiting: "Waiting on you",
    yourTurn: "Your turn",
    idle: "Idle",
    last4h: "Last 4 hours",
    today: "Today",
    last7d: "Last 7 days",
    span4h: "4h",
    spanToday: "Today",
    span7d: "7d",
    trayWaiting: "{n} waiting",
    trayNothing: "nothing waiting",
    trayToday: "${cost} today",
    trayOpen: "Open Specola",
    trayQuit: "Quit",
    pinned: "Pinned",
    unpinned: "Unpinned",
    settings: "Settings",
    back: "‹ Back",
    waitingOnYou: "waiting on you",
    sessionsOne: "{n} session",
    sessionsMany: "{n} sessions",
    quiet: "{n} quiet",
    yourTurnCount: "{n} your turn",
    waitingShare: "{p}% waiting on you",
    now: "now",
    sectionSessions: "Sessions",
    pin: "Pin",
    pinTitle: "Pin to top",
    unpinTitle: "Unpin",
    dismiss: "Dismiss",
    dismissTitle: "Dismiss until it acts again",
    keepAbove: "Keep the panel above other windows",
    emptyTitle: "No sessions in this window.",
    emptyBody: "Start Claude Code in a project, or widen the window below.",
    notifications: "Notifications",
    projectsShown: "Projects shown",
    language: "Language",
    notDetected: "Not detected",
    installTitle: "Install hooks to track live state",
    installNote:
      "States read <em>idle</em> until these run. Names and cost are already live. Paste into {path} — Specola never edits it for you.",
    copySnippet: "Copy snippet",
    jumpToPane: "Jump to pane",
    focusTab: "Focus tab",
    openVsCode: "Open in VS Code",
    copyResume: "Copy resume command",
    pasteToResume: "{label} — paste to resume",
    focusUnavailable: "Focus unavailable — resume with: {resume}",
    focusFailed: "Focus failed",
    settingNotSaved: "Setting not saved",
    snippetCopied: "Snippet copied — paste into settings.json",
    copyFailed: "Copy failed — select the snippet manually",
    hooksDetected: "Hooks detected ✓",
    about: "About",
    tagline: "Watches your Claude Code sessions.",
    installUpdate: "Update to v{version}",
    updateInstalling: "Downloading update…",
    updateFailed: "Update failed",
    updatedTo: "Updated to v{version}",
    launchAtLogin: "Launch at login",
    general: "General",
    starOnGithub: "Star on GitHub",
    dismissReadLabel: "Dismiss marks read",
    shareMyDay: "Share my day",
    dayCopied: "Day card copied — paste it anywhere",
    dayCopyFailed: "Couldn't copy the card",
    daySaved: "Day card saved to Downloads",
    shareNudge: "Good run today — share your day? Tap ↗ above",
    download: "Download",
    tokens: "tokens",
  },
  pt: {
    working: "Trabalhando",
    waiting: "Esperando você",
    yourTurn: "Sua vez",
    idle: "Parado",
    last4h: "Últimas 4 horas",
    today: "Hoje",
    last7d: "Últimos 7 dias",
    span4h: "4h",
    spanToday: "Hoje",
    span7d: "7d",
    trayWaiting: "{n} esperando por você",
    trayNothing: "nada esperando",
    trayToday: "${cost} hoje",
    trayOpen: "Abrir Specola",
    trayQuit: "Sair",
    pinned: "Fixado",
    unpinned: "Solto",
    settings: "Ajustes",
    back: "‹ Voltar",
    waitingOnYou: "esperando você",
    sessionsOne: "{n} sessão",
    sessionsMany: "{n} sessões",
    quiet: "{n} quietas",
    yourTurnCount: "{n} na sua vez",
    waitingShare: "{p}% esperando você",
    now: "agora",
    sectionSessions: "Sessões",
    pin: "Fixar",
    pinTitle: "Fixar no topo",
    unpinTitle: "Soltar",
    dismiss: "Dispensar",
    dismissTitle: "Dispensar até agir de novo",
    keepAbove: "Manter o painel acima das outras janelas",
    emptyTitle: "Nenhuma sessão nesta janela.",
    emptyBody: "Inicie o Claude Code em um projeto, ou amplie a janela abaixo.",
    notifications: "Notificações",
    projectsShown: "Projetos exibidos",
    language: "Idioma",
    notDetected: "Não detectado",
    installTitle: "Instale os hooks para acompanhar o estado ao vivo",
    installNote:
      "Os estados aparecem como <em>parados</em> até os hooks rodarem. Nomes e custo já estão ao vivo. Cole em {path} — o Specola nunca edita o arquivo por você.",
    copySnippet: "Copiar trecho",
    jumpToPane: "Ir para o painel",
    focusTab: "Focar a aba",
    openVsCode: "Abrir no VS Code",
    copyResume: "Copiar comando de retomada",
    pasteToResume: "{label} — cole para retomar",
    focusUnavailable: "Foco indisponível — retome com: {resume}",
    focusFailed: "Falha ao focar",
    settingNotSaved: "Ajuste não salvo",
    snippetCopied: "Trecho copiado — cole no settings.json",
    copyFailed: "Falha ao copiar — selecione o trecho manualmente",
    hooksDetected: "Hooks detectados ✓",
    about: "Sobre",
    tagline: "Vigia suas sessões do Claude Code.",
    installUpdate: "Atualizar para v{version}",
    updateInstalling: "Baixando atualização…",
    updateFailed: "Falha na atualização",
    updatedTo: "Atualizado para v{version}",
    launchAtLogin: "Abrir ao iniciar sessão",
    general: "Geral",
    starOnGithub: "Estrela no GitHub",
    dismissReadLabel: "Dispensar marca lida",
    shareMyDay: "Compartilhar meu dia",
    dayCopied: "Cartão do dia copiado — cole onde quiser",
    dayCopyFailed: "Não foi possível copiar o cartão",
    daySaved: "Cartão do dia salvo em Downloads",
    shareNudge: "Belo dia — compartilhe? Toque em ↗ acima",
    download: "Baixar",
    tokens: "tokens",
  },
  es: {
    working: "Trabajando",
    waiting: "Esperándote",
    yourTurn: "Tu turno",
    idle: "Inactivo",
    last4h: "Últimas 4 horas",
    today: "Hoy",
    last7d: "Últimos 7 días",
    span4h: "4h",
    spanToday: "Hoy",
    span7d: "7d",
    trayWaiting: "{n} esperando",
    trayNothing: "nada esperando",
    trayToday: "${cost} hoy",
    trayOpen: "Abrir Specola",
    trayQuit: "Salir",
    pinned: "Fijado",
    unpinned: "Suelto",
    settings: "Ajustes",
    back: "‹ Atrás",
    waitingOnYou: "esperándote",
    sessionsOne: "{n} sesión",
    sessionsMany: "{n} sesiones",
    quiet: "{n} en calma",
    yourTurnCount: "{n} tu turno",
    waitingShare: "{p}% esperándote",
    now: "ahora",
    sectionSessions: "Sesiones",
    pin: "Fijar",
    pinTitle: "Fijar arriba",
    unpinTitle: "Soltar",
    dismiss: "Descartar",
    dismissTitle: "Descartar hasta que actúe de nuevo",
    keepAbove: "Mantener el panel sobre las demás ventanas",
    emptyTitle: "No hay sesiones en esta ventana.",
    emptyBody: "Inicia Claude Code en un proyecto, o amplía la ventana abajo.",
    notifications: "Notificaciones",
    projectsShown: "Proyectos mostrados",
    language: "Idioma",
    notDetected: "No detectado",
    installTitle: "Instala los hooks para seguir el estado en vivo",
    installNote:
      "Los estados aparecen como <em>inactivos</em> hasta que se ejecuten. Los nombres y el costo ya están en vivo. Pega en {path} — Specola nunca lo edita por ti.",
    copySnippet: "Copiar fragmento",
    jumpToPane: "Ir al panel",
    focusTab: "Enfocar pestaña",
    openVsCode: "Abrir en VS Code",
    copyResume: "Copiar comando de reanudación",
    pasteToResume: "{label} — pega para reanudar",
    focusUnavailable: "Enfoque no disponible — reanuda con: {resume}",
    focusFailed: "Falló el enfoque",
    settingNotSaved: "Ajuste no guardado",
    snippetCopied: "Fragmento copiado — pégalo en settings.json",
    copyFailed: "Falló la copia — selecciona el fragmento manualmente",
    hooksDetected: "Hooks detectados ✓",
    about: "Acerca de",
    tagline: "Vigila tus sesiones de Claude Code.",
    installUpdate: "Actualizar a v{version}",
    updateInstalling: "Descargando actualización…",
    updateFailed: "Falló la actualización",
    updatedTo: "Actualizado a v{version}",
    launchAtLogin: "Abrir al iniciar sesión",
    general: "General",
    starOnGithub: "Estrella en GitHub",
    dismissReadLabel: "Descartar marca leído",
    shareMyDay: "Compartir mi día",
    dayCopied: "Tarjeta del día copiada — pégala donde quieras",
    dayCopyFailed: "No se pudo copiar la tarjeta",
    daySaved: "Tarjeta del día guardada en Descargas",
    shareNudge: "Buen día — ¿compartirlo? Toca ↗ arriba",
    download: "Descargar",
    tokens: "tokens",
  },
  fr: {
    working: "En cours",
    waiting: "En attente de vous",
    yourTurn: "À vous",
    idle: "Inactif",
    last4h: "4 dernières heures",
    today: "Aujourd'hui",
    last7d: "7 derniers jours",
    span4h: "4h",
    spanToday: "Auj.",
    span7d: "7d",
    trayWaiting: "{n} en attente",
    trayNothing: "rien en attente",
    trayToday: "${cost} aujourd'hui",
    trayOpen: "Ouvrir Specola",
    trayQuit: "Quitter",
    pinned: "Épinglé",
    unpinned: "Détaché",
    settings: "Réglages",
    back: "‹ Retour",
    waitingOnYou: "en attente de vous",
    sessionsOne: "{n} session",
    sessionsMany: "{n} sessions",
    quiet: "{n} au calme",
    yourTurnCount: "{n} à vous",
    waitingShare: "{p}% en attente de vous",
    now: "maintenant",
    sectionSessions: "Sessions",
    pin: "Épingler",
    pinTitle: "Épingler en haut",
    unpinTitle: "Détacher",
    dismiss: "Ignorer",
    dismissTitle: "Ignorer jusqu'à la prochaine action",
    keepAbove: "Garder le panneau au-dessus des autres fenêtres",
    emptyTitle: "Aucune session dans cette fenêtre.",
    emptyBody: "Lancez Claude Code dans un projet, ou élargissez la fenêtre ci-dessous.",
    notifications: "Notifications",
    projectsShown: "Projets affichés",
    language: "Langue",
    notDetected: "Non détecté",
    installTitle: "Installez les hooks pour suivre l'état en direct",
    installNote:
      "Les états affichent <em>inactif</em> jusqu'à leur exécution. Les noms et le coût sont déjà en direct. Collez dans {path} — Specola ne le modifie jamais pour vous.",
    copySnippet: "Copier l'extrait",
    jumpToPane: "Aller au panneau",
    focusTab: "Activer l'onglet",
    openVsCode: "Ouvrir dans VS Code",
    copyResume: "Copier la commande de reprise",
    pasteToResume: "{label} — collez pour reprendre",
    focusUnavailable: "Focus indisponible — reprenez avec : {resume}",
    focusFailed: "Échec du focus",
    settingNotSaved: "Réglage non enregistré",
    snippetCopied: "Extrait copié — collez dans settings.json",
    copyFailed: "Échec de la copie — sélectionnez l'extrait manuellement",
    hooksDetected: "Hooks détectés ✓",
    about: "À propos",
    tagline: "Veille sur vos sessions Claude Code.",
    installUpdate: "Mettre à jour vers v{version}",
    updateInstalling: "Téléchargement de la mise à jour…",
    updateFailed: "Échec de la mise à jour",
    updatedTo: "Mis à jour vers v{version}",
    launchAtLogin: "Lancer à la connexion",
    general: "Général",
    starOnGithub: "Étoile sur GitHub",
    dismissReadLabel: "Ignorer marque lu",
    shareMyDay: "Partager ma journée",
    dayCopied: "Carte du jour copiée — collez-la où vous voulez",
    dayCopyFailed: "Impossible de copier la carte",
    daySaved: "Carte du jour enregistrée dans Téléchargements",
    shareNudge: "Belle journée — la partager ? Touchez ↗ en haut",
    download: "Télécharger",
    tokens: "tokens",
  },
  de: {
    working: "Arbeitet",
    waiting: "Wartet auf dich",
    yourTurn: "Du bist dran",
    idle: "Inaktiv",
    last4h: "Letzte 4 Stunden",
    today: "Heute",
    last7d: "Letzte 7 Tage",
    span4h: "4h",
    spanToday: "Heute",
    span7d: "7d",
    trayWaiting: "{n} warten",
    trayNothing: "nichts wartet",
    trayToday: "${cost} heute",
    trayOpen: "Specola öffnen",
    trayQuit: "Beenden",
    pinned: "Angeheftet",
    unpinned: "Gelöst",
    settings: "Einstellungen",
    back: "‹ Zurück",
    waitingOnYou: "wartet auf dich",
    sessionsOne: "{n} Sitzung",
    sessionsMany: "{n} Sitzungen",
    quiet: "{n} ruhig",
    yourTurnCount: "{n} du bist dran",
    waitingShare: "{p}% wartet auf dich",
    now: "jetzt",
    sectionSessions: "Sitzungen",
    pin: "Anheften",
    pinTitle: "Oben anheften",
    unpinTitle: "Lösen",
    dismiss: "Ausblenden",
    dismissTitle: "Ausblenden, bis wieder aktiv",
    keepAbove: "Panel über anderen Fenstern halten",
    emptyTitle: "Keine Sitzungen in diesem Zeitraum.",
    emptyBody: "Starte Claude Code in einem Projekt oder erweitere unten das Fenster.",
    notifications: "Benachrichtigungen",
    projectsShown: "Angezeigte Projekte",
    language: "Sprache",
    notDetected: "Nicht erkannt",
    installTitle: "Hooks installieren, um den Live-Status zu verfolgen",
    installNote:
      "Zustände zeigen <em>inaktiv</em>, bis die Hooks laufen. Namen und Kosten sind bereits live. Füge in {path} ein — Specola bearbeitet die Datei nie für dich.",
    copySnippet: "Snippet kopieren",
    jumpToPane: "Zum Bereich springen",
    focusTab: "Tab fokussieren",
    openVsCode: "In VS Code öffnen",
    copyResume: "Fortsetzungsbefehl kopieren",
    pasteToResume: "{label} — zum Fortsetzen einfügen",
    focusUnavailable: "Fokus nicht verfügbar — fortsetzen mit: {resume}",
    focusFailed: "Fokus fehlgeschlagen",
    settingNotSaved: "Einstellung nicht gespeichert",
    snippetCopied: "Snippet kopiert — in settings.json einfügen",
    copyFailed: "Kopieren fehlgeschlagen — Snippet manuell auswählen",
    hooksDetected: "Hooks erkannt ✓",
    about: "Über",
    tagline: "Wacht über deine Claude-Code-Sitzungen.",
    installUpdate: "Auf v{version} aktualisieren",
    updateInstalling: "Update wird heruntergeladen…",
    updateFailed: "Update fehlgeschlagen",
    updatedTo: "Aktualisiert auf v{version}",
    launchAtLogin: "Beim Anmelden starten",
    general: "Allgemein",
    starOnGithub: "Stern auf GitHub",
    dismissReadLabel: "Ausblenden als gelesen",
    shareMyDay: "Meinen Tag teilen",
    dayCopied: "Tageskarte kopiert — überall einfügen",
    dayCopyFailed: "Karte konnte nicht kopiert werden",
    daySaved: "Tageskarte in Downloads gespeichert",
    shareNudge: "Guter Tag — teilen? Tippe oben auf ↗",
    download: "Herunterladen",
    tokens: "Tokens",
  },
  ru: {
    working: "Работает",
    waiting: "Ждёт вас",
    yourTurn: "Ваш ход",
    idle: "Простой",
    last4h: "Последние 4 часа",
    today: "Сегодня",
    last7d: "Последние 7 дней",
    span4h: "4h",
    spanToday: "Сегодня",
    span7d: "7d",
    trayWaiting: "{n} ждут вас",
    trayNothing: "ничего не ждёт",
    trayToday: "${cost} сегодня",
    trayOpen: "Открыть Specola",
    trayQuit: "Выход",
    pinned: "Закреплено",
    unpinned: "Откреплено",
    settings: "Настройки",
    back: "‹ Назад",
    waitingOnYou: "ждёт вас",
    sessionsOne: "{n} сессия",
    sessionsMany: "{n} сессий",
    quiet: "{n} в покое",
    yourTurnCount: "{n} ваш ход",
    waitingShare: "{p}% ждёт вас",
    now: "сейчас",
    sectionSessions: "Сессии",
    pin: "Закрепить",
    pinTitle: "Закрепить вверху",
    unpinTitle: "Открепить",
    dismiss: "Скрыть",
    dismissTitle: "Скрыть до следующего действия",
    keepAbove: "Держать панель поверх других окон",
    emptyTitle: "Нет сессий в этом окне.",
    emptyBody: "Запустите Claude Code в проекте или расширьте окно ниже.",
    notifications: "Уведомления",
    projectsShown: "Показанные проекты",
    language: "Язык",
    notDetected: "Не обнаружено",
    installTitle: "Установите hooks для отслеживания состояния в реальном времени",
    installNote:
      "Состояния показывают <em>простой</em>, пока hooks не запущены. Имена и стоимость уже актуальны. Вставьте в {path} — Specola никогда не редактирует его за вас.",
    copySnippet: "Копировать сниппет",
    jumpToPane: "Перейти к панели",
    focusTab: "Фокус на вкладку",
    openVsCode: "Открыть в VS Code",
    copyResume: "Копировать команду возобновления",
    pasteToResume: "{label} — вставьте, чтобы возобновить",
    focusUnavailable: "Фокус недоступен — возобновите: {resume}",
    focusFailed: "Не удалось сфокусировать",
    settingNotSaved: "Настройка не сохранена",
    snippetCopied: "Сниппет скопирован — вставьте в settings.json",
    copyFailed: "Не удалось скопировать — выделите сниппет вручную",
    hooksDetected: "Hooks обнаружены ✓",
    about: "О программе",
    tagline: "Следит за вашими сессиями Claude Code.",
    installUpdate: "Обновить до v{version}",
    updateInstalling: "Загрузка обновления…",
    updateFailed: "Не удалось обновить",
    updatedTo: "Обновлено до v{version}",
    launchAtLogin: "Запускать при входе",
    general: "Общие",
    starOnGithub: "Звезда на GitHub",
    dismissReadLabel: "Скрыть как прочитано",
    shareMyDay: "Поделиться днём",
    dayCopied: "Карточка дня скопирована — вставьте куда угодно",
    dayCopyFailed: "Не удалось скопировать карточку",
    daySaved: "Карточка дня сохранена в Загрузки",
    shareNudge: "Хороший день — поделиться? Нажмите ↗ вверху",
    download: "Скачать",
    tokens: "токены",
  },
  zh: {
    working: "运行中",
    waiting: "等待你",
    yourTurn: "轮到你",
    idle: "空闲",
    last4h: "最近 4 小时",
    today: "今天",
    last7d: "最近 7 天",
    span4h: "4h",
    spanToday: "今天",
    span7d: "7d",
    trayWaiting: "{n} 项等待",
    trayNothing: "无等待项",
    trayToday: "今日 ${cost}",
    trayOpen: "打开 Specola",
    trayQuit: "退出",
    pinned: "已置顶",
    unpinned: "未置顶",
    settings: "设置",
    back: "‹ 返回",
    waitingOnYou: "等待你",
    sessionsOne: "{n} 个会话",
    sessionsMany: "{n} 个会话",
    quiet: "{n} 个空闲",
    yourTurnCount: "{n} 个轮到你",
    waitingShare: "{p}% 等待你",
    now: "现在",
    sectionSessions: "会话",
    pin: "置顶",
    pinTitle: "置顶",
    unpinTitle: "取消置顶",
    dismiss: "忽略",
    dismissTitle: "忽略，直到再次活动",
    keepAbove: "让面板保持在其他窗口之上",
    emptyTitle: "此时段没有会话。",
    emptyBody: "在项目中启动 Claude Code，或在下方扩大窗口。",
    notifications: "通知",
    projectsShown: "显示的项目",
    language: "语言",
    notDetected: "未检测到",
    installTitle: "安装 hooks 以跟踪实时状态",
    installNote:
      "在 hooks 运行前状态显示为<em>空闲</em>。名称和成本已是实时的。粘贴到 {path} — Specola 绝不会替你编辑它。",
    copySnippet: "复制代码段",
    jumpToPane: "跳转到窗格",
    focusTab: "聚焦标签页",
    openVsCode: "在 VS Code 中打开",
    copyResume: "复制恢复命令",
    pasteToResume: "{label} — 粘贴以恢复",
    focusUnavailable: "无法聚焦 — 用此恢复：{resume}",
    focusFailed: "聚焦失败",
    settingNotSaved: "设置未保存",
    snippetCopied: "代码段已复制 — 粘贴到 settings.json",
    copyFailed: "复制失败 — 请手动选择代码段",
    hooksDetected: "已检测到 hooks ✓",
    about: "关于",
    tagline: "守望你的 Claude Code 会话。",
    installUpdate: "更新到 v{version}",
    updateInstalling: "正在下载更新…",
    updateFailed: "更新失败",
    updatedTo: "已更新至 v{version}",
    launchAtLogin: "登录时启动",
    general: "常规",
    starOnGithub: "在 GitHub 加星",
    dismissReadLabel: "忽略即已读",
    shareMyDay: "分享我的一天",
    dayCopied: "今日卡片已复制 — 随处粘贴",
    dayCopyFailed: "无法复制卡片",
    daySaved: "今日卡片已保存到下载",
    shareNudge: "今天不错 — 分享一下？点击上方 ↗",
    download: "下载",
    tokens: "tokens",
  },
  ja: {
    working: "実行中",
    waiting: "あなた待ち",
    yourTurn: "あなたの番",
    idle: "アイドル",
    last4h: "直近 4 時間",
    today: "今日",
    last7d: "過去 7 日間",
    span4h: "4h",
    spanToday: "今日",
    span7d: "7d",
    trayWaiting: "{n} 件待機中",
    trayNothing: "待機なし",
    trayToday: "本日 ${cost}",
    trayOpen: "Specola を開く",
    trayQuit: "終了",
    pinned: "固定中",
    unpinned: "固定解除",
    settings: "設定",
    back: "‹ 戻る",
    waitingOnYou: "あなた待ち",
    sessionsOne: "{n} セッション",
    sessionsMany: "{n} セッション",
    quiet: "{n} 個静か",
    yourTurnCount: "{n} 個あなたの番",
    waitingShare: "{p}% あなた待ち",
    now: "今",
    sectionSessions: "セッション",
    pin: "固定",
    pinTitle: "上部に固定",
    unpinTitle: "固定解除",
    dismiss: "非表示",
    dismissTitle: "次の動きまで非表示",
    keepAbove: "パネルを常に最前面に表示",
    emptyTitle: "この期間にセッションはありません。",
    emptyBody: "プロジェクトで Claude Code を起動するか、下でウィンドウを広げてください。",
    notifications: "通知",
    projectsShown: "表示するプロジェクト",
    language: "言語",
    notDetected: "未検出",
    installTitle: "ライブ状態を追跡するには hooks をインストール",
    installNote:
      "hooks が動くまで状態は<em>アイドル</em>と表示されます。名前とコストはすでにライブです。{path} に貼り付けてください — Specola が代わりに編集することはありません。",
    copySnippet: "スニペットをコピー",
    jumpToPane: "ペインに移動",
    focusTab: "タブをフォーカス",
    openVsCode: "VS Code で開く",
    copyResume: "再開コマンドをコピー",
    pasteToResume: "{label} — 貼り付けて再開",
    focusUnavailable: "フォーカス不可 — 再開: {resume}",
    focusFailed: "フォーカス失敗",
    settingNotSaved: "設定を保存できません",
    snippetCopied: "スニペットをコピーしました — settings.json に貼り付け",
    copyFailed: "コピー失敗 — スニペットを手動で選択",
    hooksDetected: "hooks を検出 ✓",
    about: "情報",
    tagline: "Claude Code のセッションを見守ります。",
    installUpdate: "v{version} に更新",
    updateInstalling: "アップデートをダウンロード中…",
    updateFailed: "アップデート失敗",
    updatedTo: "v{version} に更新しました",
    launchAtLogin: "ログイン時に起動",
    general: "一般",
    starOnGithub: "GitHub でスター",
    dismissReadLabel: "非表示で既読",
    shareMyDay: "今日をシェア",
    dayCopied: "今日のカードをコピーしました — どこにでも貼り付け",
    dayCopyFailed: "カードをコピーできませんでした",
    daySaved: "今日のカードをダウンロードに保存しました",
    shareNudge: "良い一日 — シェアする？上の ↗ をタップ",
    download: "ダウンロード",
    tokens: "トークン",
  },
  hi: {
    working: "काम कर रहा है",
    waiting: "आपका इंतज़ार",
    yourTurn: "आपकी बारी",
    idle: "निष्क्रिय",
    last4h: "पिछले 4 घंटे",
    today: "आज",
    last7d: "पिछले 7 दिन",
    span4h: "4h",
    spanToday: "आज",
    span7d: "7d",
    trayWaiting: "{n} प्रतीक्षारत",
    trayNothing: "कुछ भी प्रतीक्षारत नहीं",
    trayToday: "आज ${cost}",
    trayOpen: "Specola खोलें",
    trayQuit: "बाहर निकलें",
    pinned: "पिन किया",
    unpinned: "अनपिन",
    settings: "सेटिंग्स",
    back: "‹ वापस",
    waitingOnYou: "आपका इंतज़ार",
    sessionsOne: "{n} सत्र",
    sessionsMany: "{n} सत्र",
    quiet: "{n} शांत",
    yourTurnCount: "{n} आपकी बारी",
    waitingShare: "{p}% आपका इंतज़ार",
    now: "अभी",
    sectionSessions: "सत्र",
    pin: "पिन करें",
    pinTitle: "ऊपर पिन करें",
    unpinTitle: "अनपिन करें",
    dismiss: "खारिज करें",
    dismissTitle: "अगली गतिविधि तक छिपाएँ",
    keepAbove: "पैनल को अन्य विंडो के ऊपर रखें",
    emptyTitle: "इस विंडो में कोई सत्र नहीं।",
    emptyBody: "किसी प्रोजेक्ट में Claude Code शुरू करें, या नीचे विंडो चौड़ी करें।",
    notifications: "सूचनाएँ",
    projectsShown: "दिखाए गए प्रोजेक्ट",
    language: "भाषा",
    notDetected: "पता नहीं चला",
    installTitle: "लाइव स्थिति ट्रैक करने के लिए hooks इंस्टॉल करें",
    installNote:
      "hooks चलने तक स्थिति <em>निष्क्रिय</em> दिखती है। नाम और लागत पहले से लाइव हैं। {path} में पेस्ट करें — Specola इसे आपके लिए कभी संपादित नहीं करता।",
    copySnippet: "स्निपेट कॉपी करें",
    jumpToPane: "पेन पर जाएँ",
    focusTab: "टैब फोकस करें",
    openVsCode: "VS Code में खोलें",
    copyResume: "रिज़्यूम कमांड कॉपी करें",
    pasteToResume: "{label} — फिर से शुरू करने के लिए पेस्ट करें",
    focusUnavailable: "फोकस अनुपलब्ध — इससे फिर शुरू करें: {resume}",
    focusFailed: "फोकस विफल",
    settingNotSaved: "सेटिंग सहेजी नहीं गई",
    snippetCopied: "स्निपेट कॉपी हुआ — settings.json में पेस्ट करें",
    copyFailed: "कॉपी विफल — स्निपेट मैन्युअली चुनें",
    hooksDetected: "hooks मिल गए ✓",
    about: "परिचय",
    tagline: "आपके Claude Code सत्रों पर नज़र रखता है।",
    installUpdate: "v{version} में अपडेट करें",
    updateInstalling: "अपडेट डाउनलोड हो रहा है…",
    updateFailed: "अपडेट विफल",
    updatedTo: "v{version} में अपडेट किया गया",
    launchAtLogin: "लॉगिन पर लॉन्च करें",
    general: "सामान्य",
    starOnGithub: "GitHub पर स्टार",
    dismissReadLabel: "खारिज = पढ़ा हुआ",
    shareMyDay: "मेरा दिन साझा करें",
    dayCopied: "दिन का कार्ड कॉपी हुआ — कहीं भी पेस्ट करें",
    dayCopyFailed: "कार्ड कॉपी नहीं हो सका",
    daySaved: "दिन का कार्ड Downloads में सहेजा गया",
    shareNudge: "अच्छा दिन — साझा करें? ऊपर ↗ टैप करें",
    download: "डाउनलोड",
    tokens: "टोकन",
  },
  ar: {
    working: "قيد العمل",
    waiting: "بانتظارك",
    yourTurn: "دورك",
    idle: "خامل",
    last4h: "آخر 4 ساعات",
    today: "اليوم",
    last7d: "آخر 7 أيام",
    span4h: "4h",
    spanToday: "اليوم",
    span7d: "7d",
    trayWaiting: "{n} في الانتظار",
    trayNothing: "لا شيء في الانتظار",
    trayToday: "${cost} اليوم",
    trayOpen: "افتح Specola",
    trayQuit: "إنهاء",
    pinned: "مثبّت",
    unpinned: "غير مثبّت",
    settings: "الإعدادات",
    back: "رجوع ›",
    waitingOnYou: "بانتظارك",
    sessionsOne: "جلسة {n}",
    sessionsMany: "{n} جلسات",
    quiet: "{n} هادئة",
    yourTurnCount: "{n} دورك",
    waitingShare: "{p}% بانتظارك",
    now: "الآن",
    sectionSessions: "الجلسات",
    pin: "تثبيت",
    pinTitle: "تثبيت في الأعلى",
    unpinTitle: "إلغاء التثبيت",
    dismiss: "تجاهل",
    dismissTitle: "تجاهل حتى النشاط التالي",
    keepAbove: "إبقاء اللوحة فوق النوافذ الأخرى",
    emptyTitle: "لا جلسات في هذه النافذة.",
    emptyBody: "ابدأ Claude Code في مشروع، أو وسّع النافذة أدناه.",
    notifications: "الإشعارات",
    projectsShown: "المشاريع المعروضة",
    language: "اللغة",
    notDetected: "غير مكتشف",
    installTitle: "ثبّت hooks لتتبع الحالة الحيّة",
    installNote:
      "تظهر الحالات <em>خامل</em> حتى تعمل الـ hooks. الأسماء والتكلفة حيّة بالفعل. الصق في {path} — لا يعدّله Specola نيابةً عنك أبدًا.",
    copySnippet: "نسخ المقتطف",
    jumpToPane: "الانتقال إلى الجزء",
    focusTab: "تركيز التبويب",
    openVsCode: "فتح في VS Code",
    copyResume: "نسخ أمر الاستئناف",
    pasteToResume: "{label} — الصق للاستئناف",
    focusUnavailable: "التركيز غير متاح — استأنف بـ: {resume}",
    focusFailed: "فشل التركيز",
    settingNotSaved: "لم يتم حفظ الإعداد",
    snippetCopied: "تم نسخ المقتطف — الصقه في settings.json",
    copyFailed: "فشل النسخ — حدّد المقتطف يدويًا",
    hooksDetected: "تم اكتشاف hooks ✓",
    about: "حول",
    tagline: "يراقب جلسات Claude Code لديك.",
    installUpdate: "التحديث إلى v{version}",
    updateInstalling: "جارٍ تنزيل التحديث…",
    updateFailed: "فشل التحديث",
    updatedTo: "تم التحديث إلى v{version}",
    launchAtLogin: "التشغيل عند تسجيل الدخول",
    general: "عام",
    starOnGithub: "نجمة على GitHub",
    dismissReadLabel: "التجاهل كمقروء",
    shareMyDay: "شارك يومي",
    dayCopied: "تم نسخ بطاقة اليوم — الصقها أينما شئت",
    dayCopyFailed: "تعذّر نسخ البطاقة",
    daySaved: "تم حفظ بطاقة اليوم في التنزيلات",
    shareNudge: "يوم جيد — شاركه؟ اضغط ↗ في الأعلى",
    download: "تنزيل",
    tokens: "رموز",
  },
};

function detectLang(): Lang {
  const saved = localStorage.getItem("lang") as Lang | null;
  if (saved && LANGS.includes(saved)) return saved;
  const prefix = navigator.language.toLowerCase().slice(0, 2) as Lang;
  return LANGS.includes(prefix) ? prefix : "en";
}

let lang: Lang = detectLang();

function t(key: string, params?: Record<string, string | number>): string {
  let value = STRINGS[lang][key] ?? STRINGS.en[key] ?? key;
  if (params) {
    for (const [name, sub] of Object.entries(params)) {
      value = value.replace(`{${name}}`, String(sub));
    }
  }
  return value;
}

const STATE_KEY: Record<SessionState, string> = {
  Working: "working",
  WaitingOnYou: "waiting",
  YourTurn: "yourTurn",
  Idle: "idle",
};

const SPAN_KEY: Record<Span, string> = { "4h": "last4h", today: "today", week: "last7d" };

/** Backend focus labels arrive in English; map them to a translatable key. */
const FOCUS_KEY: Record<string, string> = {
  "Jump to pane": "jumpToPane",
  "Focus tab": "focusTab",
  "Open in VS Code": "openVsCode",
  "Copy resume command": "copyResume",
};
const focusLabel = (english: string) => t(FOCUS_KEY[english] ?? "copyResume");

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** One unit, the largest that fits — a watch face, not a stopwatch. */
function elapsed(seconds: number): string {
  const since = Math.max(seconds, 0);
  if (since < 60) return `${since}s`;
  if (since < 3600) return `${Math.floor(since / 60)}m`;
  if (since < 86400) return `${Math.floor(since / 3600)}h`;
  return `${Math.floor(since / 86400)}d`;
}

const money = (amount: number | null) => (amount === null ? "—" : `$${amount.toFixed(2)}`);

function axisLabel(at: number, span: Span): string {
  const moment = new Date(at * 1000);
  return span === "week"
    ? moment.toLocaleDateString(undefined, { weekday: "short" })
    : moment.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
}

function row(session: SessionView, now: number): HTMLElement {
  const node = document.createElement("div");
  node.className = `row ${TONE[session.state]}`;
  node.tabIndex = 0;
  node.dataset.id = session.id;
  node.setAttribute("role", "button");
  node.title = focusLabel(session.focus);

  // A branch and a count only earn their space where a project runs more than one
  // session — otherwise they label everything and say nothing.
  const parallel = session.siblings > 1;
  node.innerHTML = `
    <span class="dot"></span>
    <div class="row-main">
      <div class="row-top">
        <span class="project"></span>
        ${parallel ? '<span class="siblings"></span>' : ""}
        ${parallel && session.branch ? '<span class="branch"></span>' : ""}
      </div>
      <div class="row-sub">
        <span class="state"></span>
        <span class="name"></span>
      </div>
    </div>
    <div class="row-actions">
      <button type="button" class="row-action pin" data-act="pin" aria-pressed="${session.pinned}"
        aria-label="${session.pinned ? t("unpinTitle") : t("pinTitle")}"
        title="${session.pinned ? t("unpinTitle") : t("pinTitle")}">${PIN_ICON}</button>
      <button type="button" class="row-action" data-act="dismiss"
        aria-label="${t("dismissTitle")}" title="${t("dismissTitle")}">${CHECK_ICON}</button>
    </div>
    <div class="row-right">
      <div class="elapsed"></div>
      <div class="row-cost"></div>
    </div>`;
  node.classList.toggle("is-pinned", session.pinned);

  const text = (selector: string, value: string) => {
    const target = node.querySelector(selector);
    if (target) target.textContent = value;
  };
  text(".project", session.project);
  text(".siblings", `×${session.siblings}`);
  text(".branch", session.branch ?? "");
  text(".state", t(STATE_KEY[session.state]));
  text(".name", session.name);
  text(".elapsed", elapsed(now - session.since));
  text(".row-cost", money(session.cost));

  return node;
}

let sessionSig = "";

/** Everything that changes a row's identity, order, or shape — but NOT the ticking
 *  elapsed timer or cost, which update in place so an active session never forces a
 *  rebuild (a rebuild would reset the scroll position). */
function structuralSignature(snapshot: Snapshot): string {
  return snapshot.sessions
    .map((s) => [s.id, s.state, s.pinned, s.siblings, s.branch ?? "", s.name].join(""))
    .join("");
}

function refreshRow(node: HTMLElement, session: SessionView, now: number) {
  const set = (selector: string, value: string) => {
    const target = node.querySelector(selector);
    if (target) target.textContent = value;
  };
  set(".elapsed", elapsed(now - session.since));
  set(".row-cost", money(session.cost));
}

function renderSessions(snapshot: Snapshot) {
  const list = el("sessions");

  if (snapshot.sessions.length === 0) {
    if (!list.querySelector(".empty")) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.innerHTML = `<strong></strong><span></span>`;
      (empty.querySelector("strong") as HTMLElement).textContent = t("emptyTitle");
      (empty.querySelector("span") as HTMLElement).textContent = t("emptyBody");
      list.replaceChildren(empty);
    }
    sessionSig = "";
    return;
  }

  const sig = structuralSignature(snapshot);
  if (sig === sessionSig && list.querySelector(".row")) {
    // Structure unchanged — only the ticking fields move. Update them in place and
    // leave the DOM (and the scroll position, and any focus) alone.
    for (const session of snapshot.sessions) {
      const node = list.querySelector<HTMLElement>(`[data-id="${CSS.escape(session.id)}"]`);
      if (node) refreshRow(node, session, snapshot.now);
    }
    return;
  }

  sessionSig = sig;
  const focused = (document.activeElement as HTMLElement | null)?.dataset?.id;
  const scroll = list.scrollTop;
  list.replaceChildren(...snapshot.sessions.map((session) => row(session, snapshot.now)));
  list.scrollTop = scroll;
  if (focused) list.querySelector<HTMLElement>(`[data-id="${CSS.escape(focused)}"]`)?.focus();
}

function renderLane(snapshot: Snapshot, span: Span) {
  const lane = el("segments");
  lane.replaceChildren();
  for (const segment of snapshot.segments) {
    const band = document.createElement("span");
    band.className = TONE[segment.state];
    band.style.flex = `${segment.to - segment.from} 0 0`;
    lane.append(band);
  }

  const width = snapshot.now - snapshot.from;
  const axis = el("axis");
  axis.replaceChildren();
  for (const fraction of [0, 1 / 3, 2 / 3]) {
    const tick = document.createElement("span");
    tick.style.left = `${fraction * 100}%`;
    tick.textContent = axisLabel(snapshot.from + width * fraction, span);
    axis.append(tick);
  }
  const now = document.createElement("span");
  now.textContent = t("now");
  axis.append(now);

  el("share").textContent = t("waitingShare", { p: Math.round(snapshot.waitingShare * 100) });
}

let hooksWere: boolean | undefined;

/**
 * The install card: shown only while the hooks are missing, so its disappearance is
 * the live "detected" signal. Specola never writes settings.json — the user pastes.
 */
function renderHooks(snapshot: Snapshot) {
  if (hooksWere === false && snapshot.hooksInstalled) toast(t("hooksDetected"));
  hooksWere = snapshot.hooksInstalled;

  const existing = document.querySelector<HTMLElement>(".hook-card");
  if (snapshot.hooksInstalled) {
    existing?.remove();
    return;
  }
  if (existing) return; // already showing; don't clobber a scroll position mid-poll

  const path = `<button type="button" class="hook-path">~/.claude/settings.json</button>`;
  const card = document.createElement("section");
  card.className = "hook-card";
  card.innerHTML = `
    <div class="hook-head">
      <span class="hook-mark">${t("notDetected")}</span>
      <span class="hook-title">${t("installTitle")}</span>
    </div>
    <p class="hook-note">${t("installNote", { path })}</p>
    <pre class="hook-snippet"></pre>
    <button type="button" class="hook-copy">${t("copySnippet")}</button>`;
  (card.querySelector(".hook-snippet") as HTMLElement).textContent = snapshot.hookSnippet;
  card.querySelector(".hook-path")?.addEventListener("click", () => {
    invoke("open_settings").catch(console.error);
  });
  card.querySelector(".hook-copy")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(snapshot.hookSnippet);
      toast(t("snippetCopied"));
    } catch {
      toast(t("copyFailed"));
    }
  });
  el("sessions").after(card);
}

function render(snapshot: Snapshot, span: Span) {
  const waiting = el("waiting");
  waiting.textContent = String(snapshot.waiting);
  waiting.classList.toggle("quiet", snapshot.waiting === 0);

  const total = snapshot.sessions.length;
  const yourTurn = snapshot.sessions.filter((s) => s.state === "YourTurn").length;
  const count = t(total === 1 ? "sessionsOne" : "sessionsMany", { n: total });
  const tail =
    yourTurn > 0 ? t("yourTurnCount", { n: yourTurn }) : t("quiet", { n: total - snapshot.waiting });
  el("tally").textContent = `${count} · ${tail}`;

  el("lane-label").textContent = t(SPAN_KEY[span]);
  el("cost-label").textContent = t(SPAN_KEY[span]);
  el("cost").textContent = money(snapshot.cost);

  renderLane(snapshot, span);
  renderSessions(snapshot);
  renderHooks(snapshot);
  renderSettings(snapshot);
}

let projectSig = "";

/** The notifications switch tracks the server every poll — cheap, no flicker. */
function renderSettings(snapshot: Snapshot) {
  el("notifications-switch").setAttribute("aria-checked", String(snapshot.notifications));
  el("dismiss-read-switch").setAttribute("aria-checked", String(snapshot.dismissRead));
  if (!el("settings").hidden) renderProjects(snapshot);
}

/** Rebuild the project list only when it actually changed, so the open sheet doesn't
 *  flicker or fight a click each second. */
function renderProjects(snapshot: Snapshot) {
  const projects = [...new Set([...snapshot.sessions.map((s) => s.project), ...snapshot.hidden])].sort();
  const sig = `${projects.join("|")}::${[...snapshot.hidden].sort().join("|")}`;
  if (sig === projectSig) return;
  projectSig = sig;

  const list = el("project-list");
  list.replaceChildren();
  for (const project of projects) {
    const shown = !snapshot.hidden.includes(project);
    const item = document.createElement("button");
    item.type = "button";
    item.className = "project-item";
    item.dataset.project = project;
    item.setAttribute("aria-pressed", String(shown));
    item.innerHTML = `<span class="project-dot"></span><span class="project-name"></span>`;
    (item.querySelector(".project-name") as HTMLElement).textContent = project;
    list.append(item);
  }
}

/** Set every static string in the chrome for the current language. */
function applyStaticStrings() {
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    node.textContent = t(node.dataset.i18n as string);
  }
  const pin = el("pin-toggle");
  pin.title = t("keepAbove");
  pin.setAttribute(
    "aria-label",
    pin.getAttribute("aria-pressed") === "true" ? t("pinned") : t("unpinned"),
  );
  const settings = el("settings-toggle");
  settings.textContent = settings.getAttribute("aria-expanded") === "true" ? t("back") : t("settings");
  const select = el<HTMLSelectElement>("lang-select");
  select.value = lang;
  select.setAttribute("aria-label", t("language"));
  document.documentElement.lang = lang;
  document.documentElement.dir = RTL.has(lang) ? "rtl" : "ltr";
  const share = el("share-day");
  share.title = t("shareMyDay");
  share.setAttribute("aria-label", t("shareMyDay"));
  el("about-version").textContent = `v${__APP_VERSION__}`;
  pushTrayStrings();
}

/** The tray menu is built in Rust, which has no locale table. This is how it gets one. */
function pushTrayStrings() {
  invoke("set_tray_strings", {
    words: {
      waiting: t("trayWaiting"),
      nothing: t("trayNothing"),
      today: t("trayToday"),
      open: t("trayOpen"),
      quit: t("trayQuit"),
    },
  }).catch(console.error);
}

let span: Span = "4h";
let toastTimer: number | undefined;

/** A brief line at the foot of the panel — what a click just did. */
function toast(message: string) {
  const node = el("toast");
  node.textContent = message;
  node.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.classList.remove("show"), 3200);
}

async function jump(id: string) {
  try {
    const result = await invoke<FocusOutcome>("focus", { id });
    const label = focusLabel(result.label);
    if (result.raised) toast(label);
    else if (result.error) toast(t("focusUnavailable", { resume: result.resume ?? "" }));
    else toast(t("pasteToResume", { label }));
  } catch (error) {
    console.error(error);
    toast(t("focusFailed"));
  }
}

/** A settings mutation, then a re-poll so the change shows at once. A rejected
 *  command means the config was not written — say so, or it reverts at next launch. */
async function set(command: string, args: Record<string, unknown>) {
  try {
    await invoke(command, args);
  } catch (error) {
    console.error(error);
    toast(t("settingNotSaved"));
  }
  poll();
}

/** A privacy-safe day card — aggregate stats only, never a project name — to copy
 *  and post. Rendered at social-card size in the brand's own colors. */
let shareCanvas: HTMLCanvasElement | null = null;
let shareSnap: Snapshot | null = null;

const LANE_COLORS: Record<SessionState, string> = {
  Idle: "#3b4048",
  Working: "#6fb2c4",
  WaitingOnYou: "#f4b860",
  YourTurn: "#c89a58",
};

function formatTokens(n: number): string {
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 0 : 1).replace(/\.0$/, "") + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e5 ? 0 : 1).replace(/\.0$/, "") + "K";
  return String(n);
}

/** The message for X/WhatsApp — the same numbers the card shows, so the link isn't bare. */
function shareText(snap: Snapshot): string {
  const stats = `${snap.sessions.length} ${t("sectionSessions").toLowerCase()} · ${formatTokens(snap.tokens)} ${t("tokens")} · ${t("waitingShare", { p: Math.round(snap.waitingShare * 100) })}`;
  return `Claude Code 🦉\n${stats}\n\nvia Specola — github.com/ronaldoscotti/specola`;
}

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** The day as a branded card: the owl wordmark, the activity graph, sessions and tokens.
 *  Privacy-safe — aggregate numbers only, never a project name. */
async function renderDayCanvas(snap: Snapshot): Promise<HTMLCanvasElement> {
  await document.fonts.ready;
  const W = 1200;
  const H = 630;
  const PAD = 76;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d")!;

  const bg = ctx.createLinearGradient(0, 0, 0, H);
  bg.addColorStop(0, "#181a22");
  bg.addColorStop(1, "#141620");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = "#262a34";
  ctx.lineWidth = 2;
  ctx.strokeRect(1, 1, W - 2, H - 2);

  const logo = new Image();
  logo.src = "/logo.png";
  try {
    await logo.decode();
    const lh = 54;
    ctx.drawImage(logo, PAD, 62, (logo.width / logo.height) * lh, lh);
  } catch {
    ctx.fillStyle = "#eae7de";
    ctx.font = "600 46px Lora, serif";
    ctx.fillText("Specola", PAD, 106);
  }

  const date = new Date(snap.now * 1000).toLocaleDateString(lang, {
    month: "long",
    day: "numeric",
    year: "numeric",
  });
  ctx.fillStyle = "#878d9a";
  ctx.font = "400 24px 'Space Mono', monospace";
  ctx.textAlign = "right";
  ctx.fillText(date, W - PAD, 100);
  ctx.textAlign = "left";

  const laneY = 210;
  const laneH = 52;
  const laneW = W - PAD * 2;
  const span = Math.max(snap.now - snap.from, 1);
  ctx.fillStyle = "#878d9a";
  ctx.font = "500 20px 'Space Mono', monospace";
  ctx.fillText(t("today").toUpperCase(), PAD, laneY - 18);
  ctx.fillStyle = "#e7c08a";
  ctx.textAlign = "right";
  ctx.fillText(t("waitingShare", { p: Math.round(snap.waitingShare * 100) }), W - PAD, laneY - 18);
  ctx.textAlign = "left";
  roundRectPath(ctx, PAD, laneY, laneW, laneH, 9);
  ctx.fillStyle = "#20242e";
  ctx.fill();
  ctx.save();
  roundRectPath(ctx, PAD, laneY, laneW, laneH, 9);
  ctx.clip();
  for (const s of snap.segments) {
    const x = PAD + ((s.from - snap.from) / span) * laneW;
    const w = ((s.to - s.from) / span) * laneW;
    ctx.fillStyle = LANE_COLORS[s.state] ?? "#3b4048";
    ctx.fillRect(x, laneY, Math.max(w, 1), laneH);
  }
  ctx.restore();
  ctx.fillStyle = "#565c6a";
  ctx.font = "400 18px 'Space Mono', monospace";
  ctx.textAlign = "right";
  ctx.fillText(t("now"), W - PAD, laneY + laneH + 30);
  ctx.textAlign = "left";

  const stats: [string, string][] = [
    [String(snap.sessions.length), t("sectionSessions").toLowerCase()],
    [formatTokens(snap.tokens), t("tokens")],
  ];
  stats.forEach(([value, label], i) => {
    const x = PAD + i * (laneW / 2);
    ctx.fillStyle = "#f4b860";
    ctx.font = "700 90px 'Space Mono', monospace";
    ctx.fillText(value, x, 440);
    ctx.fillStyle = "#b8bec9";
    ctx.font = "400 28px Lora, serif";
    ctx.fillText(label, x, 486);
  });

  ctx.fillStyle = "#565c6a";
  ctx.font = "400 22px 'Space Mono', monospace";
  ctx.fillText("github.com/ronaldoscotti/specola", PAD, H - 46);

  return canvas;
}

async function shareDay() {
  const snap = await invoke<Snapshot>("snapshot", { span: "today" }).catch(() => lastSnapshot);
  if (!snap) return;
  shareSnap = snap;
  shareCanvas = await renderDayCanvas(snap);
  el<HTMLImageElement>("share-preview").src = shareCanvas.toDataURL("image/png");
  const sheet = el("share-sheet");
  sheet.hidden = false;
  requestAnimationFrame(() => sheet.classList.add("open"));
}

function closeShareSheet() {
  const sheet = el("share-sheet");
  sheet.classList.remove("open");
  setTimeout(() => (sheet.hidden = true), 200);
}

async function downloadCard() {
  if (!shareCanvas) return;
  const blob = await new Promise<Blob | null>((r) => shareCanvas!.toBlob(r, "image/png"));
  if (!blob) return;
  try {
    const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
    await invoke("save_day_card", { bytes });
    const button = el("share-download");
    button.classList.add("done");
    toast(t("daySaved"));
    setTimeout(() => button.classList.remove("done"), 1800);
  } catch (error) {
    console.error(error);
    toast(t("dayCopyFailed"));
  }
}

function shareTo(base: string) {
  if (!shareSnap) return;
  invoke("open_url", { url: base + encodeURIComponent(shareText(shareSnap)) }).catch(console.error);
}

let pendingUpdate: Update | null = null;

/** Silent on launch: a found update surfaces as an About affordance, never an
 *  auto-install. No endpoint reachable (dev, offline) degrades to nothing. */
async function checkForUpdates() {
  try {
    const update = await check();
    if (!update) return;
    pendingUpdate = update;
    const button = el("about-update");
    button.textContent = t("installUpdate", { version: update.version });
    button.hidden = false;
    const settings = el("settings-toggle");
    settings.dataset.update = "";
    settings.title = t("installUpdate", { version: update.version });
  } catch (error) {
    console.error(error);
  }
}

async function installUpdate() {
  if (!pendingUpdate) return;
  toast(t("updateInstalling"));
  try {
    await pendingUpdate.downloadAndInstall();
    await relaunch();
  } catch (error) {
    console.error(error);
    toast(t("updateFailed"));
  }
}

let inflight = false;
let lastSnapshot: Snapshot | undefined;

async function poll() {
  if (inflight) return;
  inflight = true;
  try {
    lastSnapshot = await invoke<Snapshot>("snapshot", { span });
    render(lastSnapshot, span);
  } catch (error) {
    console.error(error);
  } finally {
    inflight = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  if (navigator.userAgent.includes("Mac")) document.body.classList.add("mac");
  applyStaticStrings();

  el("spans").addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-span]");
    if (!button) return;
    span = button.dataset.span as Span;
    for (const other of el("spans").querySelectorAll("button")) {
      other.classList.toggle("on", other === button);
    }
    poll();
  });

  const activate = (event: Event) => {
    const target = event.target as HTMLElement;
    const action = target.closest<HTMLElement>("[data-act]");
    const row = target.closest<HTMLElement>(".row[data-id]");
    const id = row?.dataset.id;
    if (!id) return;

    // A pin/dismiss control acts on the session without jumping to it.
    if (action?.dataset.act === "pin") {
      set("set_pinned", { id, pinned: action.getAttribute("aria-pressed") !== "true" });
    } else if (action?.dataset.act === "dismiss") {
      set("dismiss", { id });
    } else {
      jump(id);
    }
  };
  el("sessions").addEventListener("click", activate);
  el("sessions").addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate(event);
    }
  });

  el("settings-toggle").addEventListener("click", () => {
    const sheet = el("settings");
    const opening = sheet.hidden;
    sheet.hidden = !opening;
    const button = el("settings-toggle");
    button.setAttribute("aria-expanded", String(opening));
    button.textContent = opening ? t("back") : t("settings");
    if (opening && lastSnapshot) renderProjects(lastSnapshot);
  });

  el("pin-toggle").addEventListener("click", (event) => {
    const button = event.currentTarget as HTMLElement;
    const pinned = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(pinned));
    button.setAttribute("aria-label", pinned ? t("pinned") : t("unpinned"));
    invoke("set_window_pinned", { pinned }).catch(console.error);
  });

  el("share-day").addEventListener("click", shareDay);
  el("share-close").addEventListener("click", closeShareSheet);
  el("share-download").addEventListener("click", downloadCard);
  el("share-x").addEventListener("click", () => shareTo("https://x.com/intent/tweet?text="));
  el("share-whatsapp").addEventListener("click", () => shareTo("https://wa.me/?text="));

  el("lang-select").addEventListener("change", (event) => {
    const value = (event.target as HTMLSelectElement).value as Lang;
    if (value === lang) return;
    lang = value;
    localStorage.setItem("lang", lang);
    projectSig = "";
    sessionSig = "";
    applyStaticStrings();
    if (lastSnapshot) render(lastSnapshot, span);
  });

  el("about-link").addEventListener("click", () => {
    invoke("open_url", { url: "https://ronaldoscotti.com" }).catch(console.error);
  });

  el("about-star").addEventListener("click", () => {
    invoke("open_url", { url: "https://github.com/ronaldoscotti/specola" }).catch(console.error);
  });

  el("about-update").addEventListener("click", installUpdate);

  el("notifications-switch").addEventListener("click", (event) => {
    const on = (event.currentTarget as HTMLElement).getAttribute("aria-checked") !== "true";
    set("set_notifications", { on });
  });

  el("autostart-switch").addEventListener("click", async (event) => {
    const button = event.currentTarget as HTMLElement;
    const on = button.getAttribute("aria-checked") !== "true";
    try {
      await (on ? enable() : disable());
      button.setAttribute("aria-checked", String(on));
    } catch (error) {
      console.error(error);
    }
  });

  el("dismiss-read-switch").addEventListener("click", (event) => {
    const on = (event.currentTarget as HTMLElement).getAttribute("aria-checked") !== "true";
    set("set_dismiss_read", { on });
  });

  el("project-list").addEventListener("click", (event) => {
    const item = (event.target as HTMLElement).closest<HTMLElement>("[data-project]");
    if (!item?.dataset.project) return;
    set("set_project_hidden", {
      project: item.dataset.project,
      hidden: item.getAttribute("aria-pressed") === "true",
    });
  });

  // ponytail: polling, not a filesystem watcher — the panel is ~10 rows and the
  // scanner only reads the bytes appended since the last tick. Revisit if that changes.
  setInterval(poll, 1000);
  poll();
  checkForUpdates();
  isEnabled()
    .then((on) => el("autostart-switch").setAttribute("aria-checked", String(on)))
    .catch(console.error);

  // A relaunch after an update is otherwise indistinguishable from any other
  // launch. Null means a fresh install, which has nothing to announce.
  const previousVersion = localStorage.getItem("specola.version");
  localStorage.setItem("specola.version", __APP_VERSION__);
  const justUpdated = previousVersion !== null && previousVersion !== __APP_VERSION__;
  if (justUpdated) toast(t("updatedTo", { version: __APP_VERSION__ }));

  // Referral loop: a single, dismissible nudge to share the day — only after the
  // tool has proven itself over a few launches, and only on a day with real activity.
  const launches = Number(localStorage.getItem("specola.launches") ?? "0") + 1;
  localStorage.setItem("specola.launches", String(launches));
  if (!justUpdated && launches >= 3 && localStorage.getItem("specola.shareNudged") !== "1") {
    setTimeout(() => {
      if ((lastSnapshot?.sessions.length ?? 0) > 0) {
        localStorage.setItem("specola.shareNudged", "1");
        toast(t("shareNudge"));
      }
    }, 2500);
  }
});
