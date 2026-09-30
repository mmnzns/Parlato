// Parlato: which OS the app runs on. WKWebView (macOS) reports "Macintosh"
// in its user agent, WebView2 (Windows) reports "Windows".
export const isMac =
  typeof navigator !== "undefined" && /Macintosh/.test(navigator.userAgent);
