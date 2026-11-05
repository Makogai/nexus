; NEXUS installer hooks (ADR-0025, ADR-0033).

; Per-user installs go to %LOCALAPPDATA%\Programs\NEXUS — never into %LOCALAPPDATA%\NEXUS, which is NEXUS's data
; folder (the database, backups, global knowledge). Program files and user data stay apart, so removing or
; replacing the program can never touch the data. Only the default is redirected; a folder the person chose stays.
!macro NSIS_HOOK_PREINSTALL
  ${If} $INSTDIR == "$LOCALAPPDATA\${PRODUCTNAME}"
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\${PRODUCTNAME}"
    CreateDirectory "$INSTDIR"
    SetOutPath "$INSTDIR"
    ; The installer created the default folder just before; remove it again only if it is empty (never data).
    RMDir "$LOCALAPPDATA\${PRODUCTNAME}"
  ${EndIf}
!macroend

; Remove what NEXUS registered for itself at run time — its notification sender (AppUserModelID) and the nexus://
; notification protocol. Nothing else: the data folder (%LOCALAPPDATA%\NEXUS) is the user's and stays.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\dev.nexus.desktop"
  DeleteRegKey HKCU "Software\Classes\nexus"
!macroend
