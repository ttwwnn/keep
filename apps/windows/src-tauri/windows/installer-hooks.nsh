; Keep's installer, around what Tauri's does.
;
; The daemon outlives the app by design: it holds every shell, and updating
; the app must not end them. Windows will not overwrite a program that is
; running, nor a DLL it has loaded, but it will let either be renamed. So an
; update moves the running daemon's files aside under a name of their own and
; puts the new ones in place: the daemon carries on from the old copy, the app
; talks to it as before, and the next daemon to start — after a reboot — is
; the new one. Copies left from earlier updates are removed when nothing runs
; them any more.

; Without a jump: on a first install there is nothing to rename, and a rename
; that fails only sets the error flag, cleared right after. (A relative jump
; over a plugin call would count the call's hidden pushes.)
!macro KEEP_MOVE_ASIDE NAME EXT
  Push $9
  System::Call 'kernel32::GetTickCount() i .r9'
  Rename "$INSTDIR\bin\${NAME}.${EXT}" "$INSTDIR\bin\${NAME}.antigo-$9.${EXT}"
  ClearErrors
  Pop $9
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; What earlier updates set aside and nothing holds any more.
  Delete "$INSTDIR\bin\*.antigo-*.*"
  !insertmacro KEEP_MOVE_ASIDE "keepd" "exe"
  !insertmacro KEEP_MOVE_ASIDE "keep" "exe"
  !insertmacro KEEP_MOVE_ASIDE "conpty" "dll"
  !insertmacro KEEP_MOVE_ASIDE "OpenConsole" "exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Removing Keep ends what it was keeping.
  Push $0
  nsExec::Exec 'taskkill /F /IM keepd.exe'
  Pop $0
  Pop $0
  Sleep 500
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\bin\*.antigo-*.*"
  RMDir "$INSTDIR\bin"
!macroend
