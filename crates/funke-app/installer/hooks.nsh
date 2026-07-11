; Funke installer hooks — wired via `bundle.windows.nsis.installerHooks` in tauri.conf.json.
;
; Tauri's installer template `!include`s this file *before* it defines its MUI pages, and MUI
; consumes (and unsets) the MUI_PAGE_CUSTOMFUNCTION_* defines at the next page macro it
; inserts. The next one is the Welcome page — so defining SHOW/LEAVE here is the supported
; seam for hanging one extra control off that page without forking the template.
;
; Ticking the box deliberately does NOT write the Run key here. The value name, its format
; and the StartupApproved companion entry are auto-launch's business (through
; tauri-plugin-autostart). The installer only leaves a marker file; funke consumes it on its
; next start, flips `autostart` in settings.json and enables it through the plugin — so the
; Settings toggle and the registry can never end up disagreeing.

!include LogicLib.nsh
!include nsDialogs.nsh

; These are spelled out rather than reusing the template's ${MANUPRODUCTKEY}/${UNINSTKEY}:
; the template defines those *after* it includes this file, and NSIS silently treats an
; unknown ${...} as literal text, so referencing them here would compile to nonsense.
!define FUNKE_MANUPRODUCTKEY "Software\klappstuhlpy\Funke"
!define FUNKE_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Funke"

!define FUNKE_APPDIR "$APPDATA\funke"
!define FUNKE_AUTOSTART_MARKER "$APPDATA\funke\.autostart-request"
!define FUNKE_RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define FUNKE_STARTUPAPPROVED "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"

Var FunkeAutostartCheckbox
Var FunkeAutostartState

; The publisher doubles as the installation's registry identity: the template records the
; install directory under Software\<publisher>\<product>, and its reinstall page reads it back
; to tell the *old* uninstaller where it lives ("uninstall.exe _?=<dir>"). Releases before
; 0.3.0 were built with a different publisher, so that lookup came back empty for them, the
; uninstaller was handed a `_?=` with nothing after it, and choosing "Uninstall" on the
; reinstall page failed with "unable to uninstall".
;
; Rebuild the entry from the Add/Remove Programs key, which is named after the *product* and
; therefore survives a publisher change. Hung off MUI's GUI-init hook (MUI owns .onGUIInit
; itself), so it runs before any page and the reinstall page sees a repaired key. Harmless
; when there is nothing to repair, and it fixes the same class of breakage if the publisher
; ever changes again.
!define MUI_CUSTOMFUNCTION_GUIINIT FunkeRepairInstallKey
Function FunkeRepairInstallKey
  ReadRegStr $0 HKCU "${FUNKE_MANUPRODUCTKEY}" ""
  ${If} $0 == ""
    ReadRegStr $1 HKCU "${FUNKE_UNINSTKEY}" "InstallLocation"
    ${If} $1 != ""
      ; InstallLocation is stored quoted; the key we are rebuilding is not.
      StrCpy $2 $1 1
      ${If} $2 == '"'
        StrLen $3 $1
        IntOp $3 $3 - 2
        StrCpy $1 $1 $3 1
      ${EndIf}
      ${If} ${FileExists} "$1\funke.exe"
        WriteRegStr HKCU "${FUNKE_MANUPRODUCTKEY}" "" "$1"
      ${EndIf}
    ${EndIf}
  ${EndIf}
FunctionEnd

!define MUI_PAGE_CUSTOMFUNCTION_SHOW FunkeWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE FunkeWelcomeLeave

; MUI builds the welcome page with nsDialogs and runs this between creating the dialog and
; showing it — so the control goes on with the same nsDialogs macros (and the same dialog
; units) MUI uses for the page's own title and text, which is what keeps it in place at any
; DPI. Its layout: image 0..109u wide, text column at 120u, dialog 193u tall.
Function FunkeWelcomeShow
  ; Default on for a first install — a launcher you have to start by hand is a launcher you
  ; forget. Default off when settings.json already exists, so re-running the installer over
  ; an existing copy can never silently re-enable an autostart the user turned off.
  ${If} ${FileExists} "${FUNKE_APPDIR}\settings.json"
    StrCpy $FunkeAutostartState 0
  ${Else}
    StrCpy $FunkeAutostartState 1
  ${EndIf}

  ; Below the page text (195u wide from 45u, 130u tall — so it ends at 175u) and above the
  ; dialog's bottom edge at 193u: overlap the label and it paints over the box. The label is
  ; a literal, not a LangString — this file is included before MUI_LANGUAGE loads, so
  ; ${LANG_*} isn't defined yet, and the app is English-only anyway.
  ${NSD_CreateCheckBox} 120u 177u 195u 12u "Start Funke when I sign in"
  Pop $FunkeAutostartCheckbox
  ; The page's background (MUI_BGCOLOR's default); set explicitly so the box doesn't paint
  ; itself onto the white page in the system button colour.
  SetCtlColors $FunkeAutostartCheckbox "" 0xFFFFFF
  ${If} $FunkeAutostartState = 1
    ${NSD_SetState} $FunkeAutostartCheckbox ${BST_CHECKED}
  ${EndIf}
FunctionEnd

Function FunkeWelcomeLeave
  ${NSD_GetState} $FunkeAutostartCheckbox $FunkeAutostartState
FunctionEnd

; A silent or updater-driven install skips the Welcome page entirely, so SHOW/LEAVE never
; run, the state stays empty, and autostart is left exactly as it was.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $FunkeAutostartState == 1
    CreateDirectory "${FUNKE_APPDIR}"
    ClearErrors
    FileOpen $0 "${FUNKE_AUTOSTART_MARKER}" w
    ${IfNot} ${Errors}
      FileWrite $0 "1"
      FileClose $0
    ${EndIf}
  ${EndIf}
!macroend

; Leaving a Run entry pointing at a deleted exe behind is the classic uninstaller sin.
; Both values are keyed by the product name — that is what auto-launch writes.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "${FUNKE_RUNKEY}" "${PRODUCTNAME}"
  DeleteRegValue HKCU "${FUNKE_STARTUPAPPROVED}" "${PRODUCTNAME}"
  Delete "${FUNKE_AUTOSTART_MARKER}"
!macroend
