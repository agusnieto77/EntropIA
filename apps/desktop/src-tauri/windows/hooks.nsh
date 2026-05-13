!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\msvcp140.dll" "$INSTDIR\msvcp140.dll"
  CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\msvcp140_1.dll" "$INSTDIR\msvcp140_1.dll"
  CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\vcomp140.dll" "$INSTDIR\vcomp140.dll"
  CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\vcruntime140.dll" "$INSTDIR\vcruntime140.dll"
  CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\vcruntime140_1.dll" "$INSTDIR\vcruntime140_1.dll"
  IfFileExists "$INSTDIR\target\release\vc-runtime\concrt140.dll" 0 +2
    CopyFiles /SILENT "$INSTDIR\target\release\vc-runtime\concrt140.dll" "$INSTDIR\concrt140.dll"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\concrt140.dll"
  Delete "$INSTDIR\msvcp140.dll"
  Delete "$INSTDIR\msvcp140_1.dll"
  Delete "$INSTDIR\vcomp140.dll"
  Delete "$INSTDIR\vcruntime140.dll"
  Delete "$INSTDIR\vcruntime140_1.dll"
!macroend
