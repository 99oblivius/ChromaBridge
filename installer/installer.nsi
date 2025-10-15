; ChromaBridge NSIS Installer Script
; VERSION should be passed via command line: makensis /DVERSION=X.Y.Z installer.nsi

!define APP_NAME "ChromaBridge"
!define COMP_NAME "ChromaBridge"
!ifndef VERSION
  !define VERSION "0.0"
!endif
!define COPYRIGHT "© 2025"
!define DESCRIPTION "Ultra-fast color blind assistance overlay"
!define INSTALLER_NAME "ChromaBridge-Setup-${VERSION}.exe"
!define APP_EXE "chromabridge.exe"
!define INSTALL_DIR "$LOCALAPPDATA\${APP_NAME}"

; Request user level (not admin)
RequestExecutionLevel user

; VERSION comes as "2025.15", need to append for VIProductVersion (requires X.Y.Z.W)
VIProductVersion "${VERSION}.0.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "CompanyName" "${COMP_NAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileDescription" "${DESCRIPTION}"
VIAddVersionKey "FileVersion" "${VERSION}"

SetCompressor /SOLID lzma
Name "${APP_NAME}"
Caption "${APP_NAME}"
OutFile "..\target\${INSTALLER_NAME}"
BrandingText "${APP_NAME}"
InstallDir "${INSTALL_DIR}"
InstallDirRegKey HKCU "Software\${APP_NAME}" ""

!include "MUI2.nsh"
!include "LogicLib.nsh"

!define MUI_ABORTWARNING
!define MUI_UNABORTWARNING

!define MUI_ICON "..\icons\icon.ico"
!define MUI_UNICON "..\icons\icon.ico"

Page custom ShowRunningAppPage LeaveRunningAppPage
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY

Var IsAppRunning
Var RunningAppDialog
Var StartMenuFolder
!define MUI_STARTMENUPAGE_DEFAULTFOLDER "${APP_NAME}"
!define MUI_STARTMENUPAGE_REGISTRY_ROOT "HKCU"
!define MUI_STARTMENUPAGE_REGISTRY_KEY "Software\${APP_NAME}"
!define MUI_STARTMENUPAGE_REGISTRY_VALUENAME "Start Menu Folder"
!insertmacro MUI_PAGE_STARTMENU Application $StartMenuFolder

!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section -MainProgram
    SetOverwrite ifnewer
    SetOutPath "$INSTDIR"
    File "..\target\release\${APP_EXE}"
    File "..\icons\icon.ico"

    SetOutPath "$APPDATA\ChromaBridge\assets\spectrums"
    File "..\assets\spectrums\*.json"

    SetOutPath "$APPDATA\ChromaBridge\assets\noise"
    File "..\assets\noise\*.png"

    SetOutPath "$INSTDIR"
    WriteRegStr HKCU "Software\${APP_NAME}" "" $INSTDIR
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayIcon" "$INSTDIR\icon.ico"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "Publisher" "${COMP_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayVersion" "${VERSION}"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "NoRepair" 1
SectionEnd

Section "Start Menu Shortcuts" SecStartMenu
    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application

    CreateDirectory "$SMPROGRAMS\$StartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\icon.ico"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\Uninstall.lnk" "$INSTDIR\Uninstall.exe"

    !insertmacro MUI_STARTMENU_WRITE_END
SectionEnd

Section /o "Desktop Shortcut" SecDesktop
    CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\icon.ico"
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
!insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} "Create Start Menu shortcuts (recommended)"
!insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Create Desktop shortcut"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function .onInit
    SectionSetFlags ${SecStartMenu} ${SF_SELECTED}
    StrCpy $IsAppRunning "0"

    nsExec::ExecToStack 'tasklist /FI "IMAGENAME eq ${APP_EXE}" /NH'
    Pop $R0
    Pop $R1
    ${If} $R0 == 0
        Push "$R1"
        Push "${APP_EXE}"
        Call StrStr
        Pop $R2
        ${If} $R2 != ""
            StrCpy $IsAppRunning "1"
        ${EndIf}
    ${EndIf}
FunctionEnd

Function StrStr
    Exch $R1
    Exch
    Exch $R0
    Push $R2
    Push $R3
    Push $R4
    Push $R5

    StrLen $R2 $R1
    StrCpy $R3 0

    loop:
        StrCpy $R4 $R0 $R2 $R3
        StrCmp $R4 $R1 done
        StrCmp $R4 "" done
        IntOp $R3 $R3 + 1
        Goto loop

    done:
        StrCpy $R0 $R4

    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

Function ShowRunningAppPage
    ${If} $IsAppRunning == "0"
        Abort
    ${EndIf}

    nsDialogs::Create 1018
    Pop $RunningAppDialog

    ${If} $RunningAppDialog == error
        Abort
    ${EndIf}

    ${NSD_CreateLabel} 0 0 100% 24u "ChromaBridge is currently running."
    Pop $0

    ${NSD_CreateLabel} 0 30u 100% 48u "The installer will close the application and restart it after installation completes. Click Next to continue or Cancel to exit the installer."
    Pop $0

    nsDialogs::Show
FunctionEnd

Function LeaveRunningAppPage
    ${If} $IsAppRunning == "1"
        DetailPrint "Closing ChromaBridge..."
        nsExec::Exec 'taskkill /F /IM "${APP_EXE}"'
        Sleep 1000
    ${EndIf}
FunctionEnd

Section Uninstall
    Delete "$INSTDIR\${APP_EXE}"
    Delete "$INSTDIR\Uninstall.exe"
    Delete "$INSTDIR\icon.ico"
    RMDir "$INSTDIR"

    Delete "$APPDATA\ChromaBridge\assets\spectrums\*.json"
    RMDir "$APPDATA\ChromaBridge\assets\spectrums"

    Delete "$APPDATA\ChromaBridge\assets\noise\*.png"
    RMDir "$APPDATA\ChromaBridge\assets\noise"

    RMDir "$APPDATA\ChromaBridge\assets"
    Delete "$APPDATA\ChromaBridge\*.db"
    Delete "$APPDATA\ChromaBridge\*.db-shm"
    Delete "$APPDATA\ChromaBridge\*.db-wal"
    Delete "$APPDATA\ChromaBridge\logs\*.log"
    RMDir "$APPDATA\ChromaBridge\logs"
    RMDir "$APPDATA\ChromaBridge"

    ; Remove Start Menu items
    !insertmacro MUI_STARTMENU_GETFOLDER Application $StartMenuFolder
    Delete "$SMPROGRAMS\$StartMenuFolder\${APP_NAME}.lnk"
    Delete "$SMPROGRAMS\$StartMenuFolder\Uninstall.lnk"
    RMDir "$SMPROGRAMS\$StartMenuFolder"

    ; Remove Desktop shortcut
    Delete "$DESKTOP\${APP_NAME}.lnk"

    ; Remove registry keys (user level)
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"
    DeleteRegKey HKCU "Software\${APP_NAME}"

    ; Remove startup entry if exists
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_NAME}"
SectionEnd
