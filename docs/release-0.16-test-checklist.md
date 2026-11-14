# Release 0.16 hand-test checklist

For Miguel, before tagging 0.16.0. Riskiest areas first: if section 1 or 2
fails, stop and report before going on.

Tags: **[Mac]**, **[Win]**, **[Both]**. Tick a box only when the expected
result happened.

## Before you start

- Install the build from the PR's CI artifacts (Mac and Windows), or build locally.
- Use a throwaway profile (a new macOS user, a Windows test account or a VM) for
  anything destructive. Never test Delete or "Delete everything" on real data.
- Have these ready: a text editor, a browser, a second app (Notes / Notepad), a
  full-screen app (Mac), a USB or Bluetooth microphone you can unplug, and an
  OpenRouter key (optional).
- Note your OS version and the build number on each report.

## 1. Existing dictation still works

- [ ] [Both] Hold to talk: hold the key, speak a sentence, let go. The text is pasted in the editor.
- [ ] [Both] Tap to start, tap to stop: same result, pasted in the app you were in.
- [ ] [Both] Hold or tap: a long press behaves like Hold to talk; a quick tap goes hands-free and a second tap stops and pastes.
- [ ] [Both] Cancel recording (Esc twice, or your Cancel shortcut): the pill closes and nothing is pasted.
- [ ] [Both] Paste last transcription and Copy last transcription shortcuts still work.
- [ ] [Mac] Paste last transcription with a shortcut using Control, Option or Shift, held while pressing: it pastes once you let go.
- [ ] [Both] A Power mode switches on for its app, and the previous settings come back afterwards.
- [ ] [Both] AI cleanup on (local model): the pasted text is cleaned up. With the model removed or the key missing, you get the "AI cleanup didn't run. Pasted as spoken." notice after the paste.
- [ ] [Both] Transcribe a short audio file: the text appears; if you block the clipboard (a clipboard manager that refuses) it says the text is saved in History.
- [ ] [Both] Settings > Privacy dropdowns (Keep audio recordings, Delete old dictations) show "After 1 day", "After 7 days"... not internal names.

## 2. Shortcuts

Open Microphone & shortcut.

- [ ] [Both] Double-tap on (default), Hold to talk: press twice quickly. Recording continues with the key released; one more press stops and pastes.
- [ ] [Both] A single quick tap in Hold to talk: stops about 0.3 s after the release, nothing is pasted by accident, and a long hold still starts instantly.
- [ ] [Both] Turn Double-tap off: a quick tap stops right away, as before.
- [ ] [Both] Double-tap in Tap to start, tap to stop and Hold or tap: behaves as the hint says (those modes already work this way).
- [ ] [Both] Custom shortcut with two modifiers alone (Ctrl+Alt on Windows, Control+Option on Mac): hold both to talk. Pressing any other key while holding drops the recording and your other shortcuts still work.
- [ ] [Both] "Tell left and right modifier keys apart": a Left Ctrl+R shortcut does not fire with Right Ctrl.
- [ ] [Win] Caps Lock as the dictation key: it works, and the note says it no longer toggles capital letters. Switch back and Caps Lock works again.
- [ ] [Mac] Caps Lock with Hold to talk: Parlato says it only works as a tap. In a tap mode it works.
- [ ] [Win] Left Win as the key: it records, the Start menu does not open. A Win+E while it is set briefly starts then cancels the recording and Explorer still opens.
- [ ] [Win] Menu key as the dictation key: it works and no context menu opens.
- [ ] [Mac] Fn / Globe as the key: it works, with the emoji/dictation note shown. If macOS opens the emoji picker, change it in System Settings > Keyboard and retest.
- [ ] [Both] Escape as the dictation key: refused until another Cancel recording shortcut is set; after setting one it works, and the recorder window closes with its own button.
- [ ] [Win] Reserved shortcuts: try Win+L, Win+D, Alt+Tab, Ctrl+C. Each shows its own message and is not saved.
- [ ] [Mac] Reserved shortcuts: try Command+Space, Command+Tab, Command+Shift+4. Each shows its own message and is not saved.
- [ ] [Both] The sidebar card and the tray menu show readable key names (for example "Left Win", "Fn / Globe key", "Right Option").
- [ ] [Both] Restart Parlato: the shortcut you set still works.

## 3. Pill

- [ ] [Mac] Open a full-screen app (a browser in full screen), dictate: the pill appears over it and the app keeps its keyboard focus (typing continues in it, the menu bar does not appear or switch).
- [ ] [Mac] Open the power mode picker on the pill over the full-screen app: it opens and closes without stealing focus.
- [ ] [Mac] Cmd-Tab while the pill is showing: it switches apps normally and the pill stays out of the app list.
- [ ] [Win] The pill still appears above the taskbar and on the screen where the mouse is, and never takes focus from the app.
- [ ] [Mac] Two screens of different sharpness: the pill is on the screen with the mouse.
- [ ] [Both] Start a dictation straight after another one: no leftover text, color or notice from the previous one.
- [ ] [Both] Block the microphone in the system settings, dictate: the pill says Parlato isn't allowed to use the microphone, with a **Fix it** link that opens the Permissions page.
- [ ] [Both] Dictate with no speech model in use: the pill failure's **Fix it** opens Speech model.
- [ ] [Both] Mic unplugged during recording: the pill says it stopped, **Fix it** opens Microphone & shortcut. A full-disk message has no Fix it link.

## 4. Review before pasting

Turn it on in Settings.

- [ ] [Both] Dictate: a small editor opens with the text. Edit a word, press Enter: the edited text is pasted in the app you were in, at the cursor.
- [ ] [Both] Shift+Enter adds a new line in the editor and does not paste.
- [ ] [Both] Esc (and Cancel): nothing is pasted; the dictation is in History.
- [ ] [Mac] The editor takes the keyboard without bringing Parlato's main window forward. After Enter, focus goes back to the original app and the text lands there. Test also from a full-screen app.
- [ ] [Win] The editor takes focus; after Enter or Esc, focus goes back to the original app and the text lands there.
- [ ] [Both] While the editor is open, pressing the dictation key does not start a new recording and tells you to paste or close the editor first.
- [ ] [Both] History shows the edited version with the "Edited" label.
- [ ] [Both] Turn the setting off: dictation pastes directly again.

## 5. Spoken commands, punctuation and snippets

Test in English, Spanish and French (set the dictation language each time).

- [ ] [Both] en: "hello new line world" gives a line break. "new paragraph" gives a blank line. "scratch that" removes the last sentence of that dictation.
- [ ] [Both] es: "nueva línea", "nuevo párrafo", "borra eso". English phrases still work in Spanish.
- [ ] [Both] fr: "nouvelle ligne", "nouveau paragraphe", "efface ça".
- [ ] [Both] Spoken commands off: the same words are typed as normal text.
- [ ] [Both] Spoken punctuation is off by default: "comma" is typed as the word. Turn it on: "comma", "period", "question mark", "open quote" become the symbols (en, es, fr).
- [ ] [Both] Dictionary > Snippets: add a trigger "my signature" with a two-line text. Say it alone: only the saved text is pasted, exactly as saved, even with AI cleanup on.
- [ ] [Both] Say the trigger inside a sentence: only those words are replaced; AI cleanup does not rewrite the snippet.
- [ ] [Both] Snippets: duplicate trigger is refused with a message; empty fields are refused; edit and delete work.
- [ ] [Both] Snippets CSV (trigger, text): export, delete a snippet, import: it comes back; existing ones are skipped.

## 6. Mac: website pairing and the Automation prompt

Use a throwaway profile or reset the Automation permission first (`tccutil reset AppleEvents`).

- [ ] [Mac] Power modes > a mode > pair with a website: add a site such as example.com. The note about the Automation prompt is shown.
- [ ] [Mac] Open that site in Safari and dictate: macOS asks to let Parlato control Safari. Allow: the mode switches on.
- [ ] [Mac] Repeat with Chrome (and one of Arc, Brave, Edge if you have them).
- [ ] [Mac] Open a different site: the mode does not apply.
- [ ] [Mac] Deny the prompt, dictate again: dictation still works without delay, and the mode shows the message with **Open Automation settings**.
- [ ] [Mac] Firefox: pair a mode with a site, dictate in Firefox: the mode switches (no new prompt). While typing in the address bar: no switch.
- [ ] [Mac] Check the log: only the site host, no full address.
- [ ] [Win] Power modes work as before (app pairing); no website option errors.

## 7. Mac: screen reading and Screen Recording

- [ ] [Mac] Permissions page: Screen Recording row shows "Not allowed yet" with the hint. Click Allow and follow the system dialog; after you allow and reopen Parlato it shows "Allowed".
- [ ] [Mac] AI cleanup on with "Let the AI see the window you're typing in" on: dictate into a document that mentions a name. The cleanup uses the name correctly.
- [ ] [Mac] The option off: no Screen Recording prompt appears and nothing is read.
- [ ] [Mac] Permission not given with the option on: dictation still pastes (as spoken), no crash or hang.
- [ ] [Win] The Permissions page shows "Reading the screen" and "Screen reading available" as before; AI cleanup with the window option works.

## 8. Microphone unplugged, disk full, permission

- [ ] [Both] Unplug the USB microphone while recording: Parlato stops and shows "The microphone stopped working during the recording." Dictate again after plugging in: it works.
- [ ] [Both] Use a slow Bluetooth microphone (just connected): Parlato does not freeze while it connects; you can still open the window and cancel.
- [ ] [Both] Fill a small disk or a limited volume while recording (use a VM): Parlato stops and says the disk is full. The app keeps running.
- [ ] [Both] Remove microphone permission (Mac: System Settings > Privacy & Security > Microphone; Windows: Settings > Privacy > Microphone) and dictate: message says Parlato isn't allowed to use the microphone, with Fix it. Give permission back: dictation works again.
- [ ] [Both] Permissions page: the Microphone button appears only when the microphone is not allowed.
- [ ] [Both] History unavailable (rename or lock the history database in the throwaway profile): a notice says History isn't available, dictation still pastes.

## 9. Setup wizard as a new user

Fresh profile or "Delete everything" in the throwaway profile.

- [ ] [Both] The first model question is "Which language do you speak?". Pick one: the matching model has a **Recommended** tag.
- [ ] [Both] The step count ("step 1 of N") matches the steps shown (Mac has the Accessibility step, Windows does not).
- [ ] [Both] **Continue** on the model step stays off until a model is in use; "Download a model, then press Use to continue." shows. **Skip** is still there.
- [ ] [Both] Download shows sizes or an indeterminate bar; when it finishes, it becomes the model in use.
- [ ] [Both] Skip the model and reach "Try it": it says dictation won't work yet, with a button back to the model step.
- [ ] [Both] The sidebar card says **Setup needed** at first, then **Almost ready** with the one missing step, then Ready with your key ("[key] to talk"). Home header matches.
- [ ] [Mac] Accessibility and microphone steps: prompts appear only when asked; Ready needs both.
- [ ] [Both] Switch the app to Spanish and French: the wizard is fully translated (tú in Spanish, vous in French), no English left over.

## 10. Models

- [ ] [Both] Speech model > a downloaded model: there is one Delete button. With no other app using the model, the dialog says "This frees X GB".
- [ ] [Both] With Talaan or Ensayo also using the model: the dialog offers **Delete from Parlato** and **Delete everywhere and free X GB**. Test on a copy only, never on real data.
- [ ] [Both] An imported GGUF model: Delete says it removes it from Parlato.
- [ ] [Both] Model downloading in another app: the card says "Downloading in another app".
- [ ] [Both] Start a download, then go to another page: the sidebar shows "Downloading [model]… N%". Come back: the card still shows progress. Cancel from the sidebar works (also for Voice detection).
- [ ] [Both] Two downloads at once: the sidebar says "2 downloads…".
- [ ] [Both] **Recommended** appears on the right model for your language, and on IBM Granite in AI cleanup.
- [ ] [Both] OpenRouter speech: add an OpenRouter key, pick Voxtral Mini Transcribe 3, press Use, dictate: text is pasted. Do the same with GPT Transcribe.
- [ ] [Both] OpenRouter with a wrong or missing key: a clear message with Fix it, not a raw error.
- [ ] [Both] Remove an account key: Parlato asks first and says which model it will switch to. Cancel keeps the key.
- [ ] [Both] AI cleanup on with no model or key: the "isn't set up yet" warning shows, with a link to get a key for online services.
- [ ] [Both] Settings failing to save (disk read-only in the throwaway profile): "Couldn't save that. Try again." and the control goes back.

## 11. History, Dictionary, tray, chart, power modes

- [ ] [Both] Home chart: words per day for 14 days and a Days / Weeks switch. Hover or focus a bar to see its date and count. An empty profile shows "Your first dictations will show up here."
- [ ] [Both] History > Edit on a dictation: change the text, Save. It shows "Edited" and "Restore original" brings the first text back. An empty text is refused.
- [ ] [Both] After saving an edit that changed words, "Add to Dictionary?" lists them. Add one: it appears in the Dictionary. Dismiss another: it does not. Nothing is added without a click.
- [ ] [Both] Dictionary > Export CSV, delete a word, Import CSV: it comes back; duplicates are skipped with "Added X, skipped Y". A non-UTF-8 or empty file gives a clear message.
- [ ] [Both] A dictionary word that starts with = (for example "=1+1") exports as text, not a formula, when opened in a spreadsheet.
- [ ] [Both] Tray icon > Recent: the last three dictations; clicking one copies it. With none, it says nothing yet. A new dictation updates the list.
- [ ] [Both] Power modes: right-click a mode (and use its menu button): Duplicate makes "(copy)", Rename renames, Delete asks first.
- [ ] [Both] Dotted background is gone in light and dark mode.
- [ ] [Both] Nothing in the app says VoiceInk, OCR, hook, Silero, enhancement or "prompt" where it should say writing style.

## 12. Updater guard and crash recovery

- [ ] [Both] With an update available, start a dictation or file transcription and click **Update and restart**: it says to wait for the dictation to finish. When it is done, the update installs and restarts.
- [ ] [Both] With nothing running, **Update and restart** installs and restarts.
- [ ] [Both] Power mode crash: set a power mode that changes the model or AI cleanup, start a dictation in its app, then force quit Parlato (Mac: Force Quit; Windows: Task Manager) mid-dictation. Reopen: your own settings are back, not the power mode's.
- [ ] [Both] Cancel a dictation in a power-mode app and fail a start (unplug the mic): your settings come back each time.
- [ ] [Both] Log size: after heavy use, the log file stays under its size limit while the app runs, and holds no dictated text, window titles or full URLs.
