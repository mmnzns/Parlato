# Installing Parlato

Parlato is free voice dictation for Windows and Mac. Hold a key, speak, let
go: your words are typed wherever your cursor is.

This guide covers installing, first setup, updates, removing Parlato, and
what to do when something doesn't work.

- [Windows](#windows)
- [Mac](#mac)
- [Troubleshooting](#troubleshooting)
- [Removing Parlato](#removing-parlato)

Parlato is free and made by one person, so it is not signed with a paid
Apple or Microsoft certificate. Windows and macOS warn about apps like that
the first time you open them. The steps below show how to get past that
warning. Everything Parlato runs is built from the public source code in
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

### 2. Install

1. Open the `.dmg` file.
2. Drag **Parlato** onto the **Applications** folder.
3. Eject the Parlato disk (click the eject button next to it in Finder).

### 3. Open Parlato the first time

macOS blocks apps from developers who don't pay for an Apple certificate.
You only need to do this once.

1. Open **Applications** and double-click **Parlato**. macOS says it can't
   check the app. Click **Done** (or **OK**).
2. Open **System Settings** > **Privacy & Security**.
3. Scroll down to **Security**. Next to the message about Parlato, click
   **Open Anyway**.
4. Enter your Mac password, then click **Open Anyway** again.

<details>
<summary>Prefer the Terminal?</summary>

This removes the "downloaded from the internet" flag, so macOS opens Parlato
without asking:

```bash
xattr -dr com.apple.quarantine /Applications/Parlato.app
```

</details>

### 4. First setup (about 2 minutes)

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

**Windows:** Settings > Apps > Installed apps > **Parlato** > Uninstall. The
uninstaller asks whether to also delete your settings, history, downloaded
models and logs; choose **No** to keep them for a later reinstall. Account
keys for online services stay in Windows Credential Manager (Control Panel >
Credential Manager) until you delete them there.

**Mac:**
1. Quit Parlato (menu bar icon > **Quit Parlato**).
2. Drag **Parlato** from Applications to the Trash.
3. To also remove your settings, history and downloaded models, delete these
   folders (in Finder, **Go** > **Go to Folder...**):
   - `~/Library/Application Support/com.craftconceptsdigital.parlato`
   - `~/Library/Logs/com.craftconceptsdigital.parlato`
4. Account keys for online services are in the **Passwords** app (or
   Keychain Access) under **Parlato**; delete them there if you want.
