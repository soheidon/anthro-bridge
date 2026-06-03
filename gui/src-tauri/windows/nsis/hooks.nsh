; NSIS_HOOK_POSTINSTALL — Write selected language to resources/ for the app to read on first launch.
; $language is the NSIS language ID (e.g. 1033 = English, 1041 = Japanese).
; We map it to the app's language code and write it to installer_lang.txt in the resources dir.

!macro NSIS_HOOK_POSTINSTALL
  ; Map NSIS language ID → app language code
  ${If} $language == "1041"
    StrCpy $0 "ja"
  ${ElseIf} $language == "2052"
    StrCpy $0 "zh-CN"
  ${ElseIf} $language == "1028"
    StrCpy $0 "zh-TW"
  ${ElseIf} $language == "1042"
    StrCpy $0 "ko"
  ${ElseIf} $language == "1036"
    StrCpy $0 "fr"
  ${Else}
    StrCpy $0 "en"
  ${EndIf}

  ; Write to resources/installer_lang.txt
  ; Create resources dir if needed
  CreateDirectory "$INSTDIR\resources"
  FileOpen $1 "$INSTDIR\resources\installer_lang.txt" w
  FileWrite $1 $0
  FileClose $1
!macroend
