; NEXUS installer hooks (ADR-0025): remove what NEXUS registered for itself at run time —
; its notification sender (AppUserModelID) and the nexus:// notification protocol. Nothing else.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\dev.nexus.desktop"
  DeleteRegKey HKCU "Software\Classes\nexus"
!macroend
