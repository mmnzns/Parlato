# Installing Parlato

Parlato is free voice dictation for Windows and Mac. Hold a key, speak, let
go: your words are typed wherever your cursor is.

This guide covers installing, first setup, updates, removing Parlato, and
what to do when something doesn't work.

- [Windows](#windows)
- [Mac](#mac)
- [Troubleshooting](#troubleshooting)
- [Removing Parlato](#removing-parlato) (leaves nothing behind)

> **Mac users:** macOS will block Parlato **twice** the first time, once
> when you open the download and once when you first open the app. Each
> time, click **Done** (not Move to Trash), then **Open Anyway** in System
> Settings > Privacy & Security. [Step by step](#2-open-the-download-and-install).

Parlato is free and made by one person, so it is not signed with a paid
Apple or Microsoft certificate. Windows and macOS warn about apps like that
the first time you open them ([why](FAQ.md#why-isnt-parlato-signed)). The
steps below show how to get past that warning. Everything Parlato runs is built from the public source code in
[this repository](https://github.com/mmnzns/Parlato).

---

## Windows

**You need:** Windows 10 (22H2) or Windows 11, on an x64 PC or a Windows on
ARM laptop. No graphics card needed.

### 1. Download

Go to the [latest release](https://github.com/mmnzns/Parlato/releases/latest)
and download one file:

| Your PC | File |
|---|---|
| Almost every Windows PC | `Parlato_x.y.z_x64-setup.exe` |
| Windows on ARM (Snapdragon laptops) | `Parlato_x.y.z_arm64-setup.exe` |

### 2. Install

1. Open the file you downloaded.
2. Windows shows **"Windows protected your PC"**. Click **More info**, then
   **Run anyway**.
3. Follow the installer. Parlato opens when it's done.

### 3. First setup (about 2 minutes)

1. **Microphone:** pick your microphone and speak: the bar should move.
2. **Speech model:** pick a model and download it once. **Parakeet Unified**
   is the best choice for English; **Parakeet Ultra** for other European
   languages.
3. **Shortcut:** the default is **Right Alt**. Hold it to talk, let go to
   stop. A short tap starts hands-free mode; tap again to stop.
4. **Try it:** dictate into the practice box.

Parlato keeps running with its icon near the clock (in the system tray, or
under the small arrow next to it). Right-click the icon for the menu,
including **Quit Parlato**.

### Updates

Parlato checks for new versions and offers to install them: click
**Update now**. Updates are signed, and Parlato refuses any update that isn't.

---

## Mac

**You need:** a Mac with Apple silicon (M1, M2, M3, M4 or newer) running
macOS 13 Ventura or later. Not sure which chip you have? Open the Apple menu
> **About This Mac** and look at **Chip**. Macs with an Intel chip are not
supported.

### 1. Download

Go to the [latest release](https://github.com/mmnzns/Parlato/releases/latest)
and download `Parlato_x.y.z_aarch64.dmg`.

### 2. Open the download and install

macOS blocks apps from developers who don't pay Apple, so it asks you to
confirm **twice**: once for the download, once the first time you open the
app. After that Parlato opens normally, and its updates don't ask again.

When you see **"Parlato" Not Opened** ("Apple could not verify..."), always
click **Done**, never **Move to Trash**, then:

1. Open **System Settings** > **Privacy & Security**.
2. Scroll down to **Security**. Next to the message about Parlato, click
   **Open Anyway**. (The button stays there for about an hour after the
   block; if it's gone, try opening the file again.)
3. Enter your Mac password, then click **Open Anyway** again.

Step by step:

1. **Open the `.dmg`** you downloaded. macOS blocks it: click **Done** and
   follow the three steps above. The Parlato window opens.
2. **Drag Parlato onto the Applications folder** in that window.
3. **Eject the Parlato disk** (the eject button next to **Parlato** in the
   Finder sidebar).
4. **Open Parlato** from Applications. macOS blocks it one more time: click
   **Done** and follow the three steps above again. Parlato opens.

<details>
<summary>Prefer the Terminal? This skips both warnings</summary>

Before opening the `.dmg`, remove its "downloaded from the internet" flag.
Everything you install from it then opens without asking:

```bash
xattr -d com.apple.quarantine ~/Downloads/Parlato_*_aarch64.dmg
```

Already installed and blocked? This does the same for the app:

```bash
xattr -dr com.apple.quarantine /Applications/Parlato.app
```

</details>

### 3. First setup (about 2 minutes)

1. **Permissions:** Parlato asks for two things, and explains both.
   - **Microphone**, to hear you. Click **Allow**, then **Allow** in the
     macOS prompt.
   - **Accessibility**, to hear your shortcut in every app and to paste the
     text. Click **Allow**, then switch **Parlato** on in System Settings >
     Privacy & Security > Accessibility. Parlato notices within a few
     seconds; no restart needed.
2. **Microphone:** pick your microphone and speak: the bar should move.
3. **Speech model:** pick a model and download it once. **Parakeet Unified**
   is the best choice for English; **Parakeet Ultra** for other European
   languages.
4. **Shortcut:** the default is **Right Option**. Hold it to talk, let go to
   stop. A short tap starts hands-free mode; tap again to stop.
   On a Mac, Option also types special characters (like é or @). Parlato
   ignores Right Option when you use it that way, but if you type accents a
   lot, **Right Command** is a good alternative.
5. **Try it:** dictate into the practice box.

Parlato keeps running with its icon in the menu bar, near the clock. Click
the icon for the menu, including **Open Parlato** and **Quit Parlato**.
Clicking Parlato in the Dock also brings the window back.

### Updates

Parlato checks for new versions and offers to install them: click
**Update now**.

### Not on Mac yet

These work on Windows and are coming to the Mac later:

- Switching a power mode automatically for a specific app or website (the
  number shortcuts and "Use everywhere else" do work)
- Letting AI cleanup read the window you're typing in
- Pausing other audio while you dictate
- The command-line AI cleanup engine

Speech and AI cleanup run on the Mac's processor for now, not its graphics
chip. Short dictations are quick; the largest Whisper models and local AI
cleanup are slower than they will be once graphics-chip support arrives.

---

## Troubleshooting

**The shortcut does nothing.**
- Mac: open **System Settings** > **Privacy & Security** > **Accessibility**
  and check Parlato is switched on. If it already is, switch it off and on
  again. Parlato's **Settings** > **Permissions** shows the status too.
- Windows: the shortcut doesn't work while a window started with
  **Run as administrator** has focus. Windows blocks that for apps that
  aren't code-signed. Click a normal window and try again.

**Recordings are silent, or the text is empty.**
- Check the right microphone is picked in **Microphone & shortcut**: speak
  and watch the bar.
- Mac: open **System Settings** > **Privacy & Security** > **Microphone** and
  check Parlato is switched on.
- Windows: open **Settings** > **Privacy & security** > **Microphone** and
  check that desktop apps may use the microphone.

**The text didn't appear where I was typing.**
- Click into the text field first, then use the shortcut.
- The last dictation is always in **History**, and the menu near the clock
  can copy or paste it again.

**Something else is wrong.**
Open **Settings** > **Log file** > **Open log folder** and attach
`parlato.log` to a [bug report](https://github.com/mmnzns/Parlato/issues).
The log records timings and errors, never what you dictate.

---

## Removing Parlato

Parlato keeps its speech models, history and settings in its own folders on
your computer. The models are the big part: each one you downloaded is
between about 75 MB and 3 GB. Removing the app alone does not remove them,
so follow every step below to leave nothing behind.

**Easiest, on both platforms (Parlato 0.9.1 or later):** open Parlato's
**Settings** > **Delete all Parlato data** > **Delete everything**. It
removes every downloaded model, your history and recordings, settings,
logs, start at login and saved account keys (and, on a Mac, Parlato's
privacy permissions), then quits Parlato. Then remove the app itself:
uninstall it on Windows (step 1 below, answer either way), or drag it to the
Trash on a Mac (step 2 below). On a Mac, also check step 6 below (Allow
in the Background). That's all.

The steps below do the same by hand, for older versions or if Parlato won't
open.

### Windows

1. Open **Settings** > **Apps** > **Installed apps**, find **Parlato**,
   click **...** > **Uninstall**.
2. The uninstaller asks **"Do you also want to delete all Parlato user
   data?"** Click **Yes** to remove everything Parlato stored:
   - speech and AI models
   - history and recordings
   - settings, prompts and power modes
   - logs
   - the "start when my computer starts" entry
   - saved account keys for online services (from 0.9.1)

   Click **No** only if you plan to reinstall and want to keep them.
3. Uninstalled a version before 0.9.1? Its account keys stay in Windows
   Credential Manager until you delete them: open **Control Panel** >
   **Credential Manager** > **Windows Credentials** and remove each entry
   ending in `.Parlato` (for example `openAIAPIKey.Parlato`).

If you clicked **No** earlier and want the data gone now, delete these two
folders (paste each path into File Explorer's address bar):

- `%APPDATA%\com.craftconceptsdigital.parlato`
- `%LOCALAPPDATA%\com.craftconceptsdigital.parlato`

### Mac

On a Mac, dragging an app to the Trash removes only the app. Parlato's
models, history and settings stay until you delete them too:

1. In Parlato, open **Settings** and switch off **Start Parlato when my
   computer starts** (if it's on). Then quit Parlato: click its menu bar
   icon > **Quit Parlato**.
2. Drag **Parlato** from **Applications** to the Trash.
3. Remove Parlato's data. Either:
   - **Finder:** choose **Go** > **Go to Folder...** (Shift+Cmd+G), paste
     each path below, and drag the folder that opens to the Trash:
     - `~/Library/Application Support/com.craftconceptsdigital.parlato`
       (models, history, recordings, settings: the big one)
     - `~/Library/Logs/com.craftconceptsdigital.parlato`
     - `~/Library/Caches/com.craftconceptsdigital.parlato`
     - `~/Library/WebKit/com.craftconceptsdigital.parlato`
   - **Or Terminal:** this moves all four to the Trash in one go:

     ```bash
     for d in "Application Support" Logs Caches WebKit; do [ -e ~/Library/"$d"/com.craftconceptsdigital.parlato ] && mv ~/Library/"$d"/com.craftconceptsdigital.parlato ~/.Trash/"parlato-$d"; done
     ```
4. Empty the Trash.
5. Optional clean-up in **System Settings** > **Privacy & Security**: in
   **Accessibility** and **Microphone**, select **Parlato** and click
   **-**.
6. If you ever turned on start at login, macOS may keep Parlato in
   **System Settings** > **General** > **Login Items & Extensions** >
   **Allow in the Background** after the app is gone. Switch **Parlato**
   off there. It's only a leftover entry; nothing is running.
7. Account keys for online services: open **Keychain Access**, search for
   **Parlato**, and delete the entries (they are named like
   `openAIAPIKey`).
