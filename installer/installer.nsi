; Color Interlacer NSIS Installer Script

!define APP_NAME "Color Interlacer"
!define COMP_NAME "Color Interlacer"
!define VERSION "0.1.0"
!define COPYRIGHT "© 2025"
!define DESCRIPTION "Ultra-fast color blind assistance overlay"
!define INSTALLER_NAME "ColorInterlacer-Setup.exe"
!define TRAY_APP_EXE "color-interlacer-tray.exe"
!define GUI_APP_EXE "color-interlacer.exe"
!define OVERLAY_APP_EXE "color-interlacer-overlay.exe"
!define INSTALL_DIR "$LOCALAPPDATA\${APP_NAME}"

; Request user level (not admin)
RequestExecutionLevel user

VIProductVersion "${VERSION}.0"
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

!define MUI_ICON "..\assets\icons\icon.ico"
!define MUI_UNICON "..\assets\icons\icon.ico"

; Custom page for running application warning
Page custom ShowRunningAppPage LeaveRunningAppPage

; Installer pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY

; Variables
Var IsAppRunning
Var RunningAppDialog

; Start Menu page
Var StartMenuFolder
!define MUI_STARTMENUPAGE_DEFAULTFOLDER "${APP_NAME}"
!define MUI_STARTMENUPAGE_REGISTRY_ROOT "HKCU"
!define MUI_STARTMENUPAGE_REGISTRY_KEY "Software\${APP_NAME}"
!define MUI_STARTMENUPAGE_REGISTRY_VALUENAME "Start Menu Folder"
!insertmacro MUI_PAGE_STARTMENU Application $StartMenuFolder

!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\${TRAY_APP_EXE}"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section -MainProgram
    SetOverwrite ifnewer
    SetOutPath "$INSTDIR"

    ; Main files
    File "..\target\release\${TRAY_APP_EXE}"
    File "..\target\release\${GUI_APP_EXE}"
    File "..\target\release\${OVERLAY_APP_EXE}"

    ; Install icon files
    SetOutPath "$INSTDIR\assets\icons"
    File "..\assets\icons\icon.ico"
    File "..\assets\icons\icon-2048.png"

    SetOutPath "$INSTDIR"

    ; Store installation folder
    WriteRegStr HKCU "Software\${APP_NAME}" "" $INSTDIR

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Add uninstall information to Add/Remove Programs (user level)
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayIcon" "$INSTDIR\${TRAY_APP_EXE}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "Publisher" "${COMP_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "DisplayVersion" "${VERSION}"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}" "NoRepair" 1
SectionEnd

Section "Start Menu Shortcuts" SecStartMenu
    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application

    CreateDirectory "$SMPROGRAMS\$StartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\${APP_NAME}.lnk" "$INSTDIR\${TRAY_APP_EXE}"
    CreateShortcut "$SMPROGRAMS\$StartMenuFolder\Uninstall.lnk" "$INSTDIR\Uninstall.exe"

    !insertmacro MUI_STARTMENU_WRITE_END
SectionEnd

Section /o "Desktop Shortcut" SecDesktop
    CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${TRAY_APP_EXE}"
SectionEnd

; Component descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
!insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} "Create Start Menu shortcuts (recommended)"
!insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Create Desktop shortcut"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; Set Start Menu section to checked by default
Function .onInit
    ; Select Start Menu section by default
    SectionSetFlags ${SecStartMenu} ${SF_SELECTED}

    ; Check if tray service, GUI, or overlay is running using tasklist
    StrCpy $IsAppRunning "0"

    ; Check tray service
    nsExec::ExecToStack 'tasklist /FI "IMAGENAME eq ${TRAY_APP_EXE}" /NH'
    Pop $R0 ; Return value
    Pop $R1 ; Output
    ${If} $R0 == 0
        ; Check if the process name appears in output using StrStr
        Push "$R1"
        Push "${TRAY_APP_EXE}"
        Call StrStr
        Pop $R2
        ${If} $R2 != ""
            StrCpy $IsAppRunning "1"
        ${EndIf}
    ${EndIf}

    ; Check GUI if not already found
    ${If} $IsAppRunning == "0"
        nsExec::ExecToStack 'tasklist /FI "IMAGENAME eq ${GUI_APP_EXE}" /NH'
        Pop $R0 ; Return value
        Pop $R1 ; Output
        ${If} $R0 == 0
            Push "$R1"
            Push "${GUI_APP_EXE}"
            Call StrStr
            Pop $R2
            ${If} $R2 != ""
                StrCpy $IsAppRunning "1"
            ${EndIf}
        ${EndIf}
    ${EndIf}

    ${If} $IsAppRunning == "0"
        nsExec::ExecToStack 'tasklist /FI "IMAGENAME eq ${OVERLAY_APP_EXE}" /NH'
        Pop $R0
        Pop $R1
        ${If} $R0 == 0
            Push "$R1"
            Push "${OVERLAY_APP_EXE}"
            Call StrStr
            Pop $R2
            ${If} $R2 != ""
                StrCpy $IsAppRunning "1"
            ${EndIf}
        ${EndIf}
    ${EndIf}
FunctionEnd

; StrStr function - searches for a substring in a string
; Input: push string, push substring
; Output: pop result (empty if not found, or substring position if found)
Function StrStr
    Exch $R1 ; substring
    Exch
    Exch $R0 ; string
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

; Show custom page if app is running
Function ShowRunningAppPage
    ${If} $IsAppRunning == "0"
        Abort ; Skip this page if app is not running
    ${EndIf}

    nsDialogs::Create 1018
    Pop $RunningAppDialog

    ${If} $RunningAppDialog == error
        Abort
    ${EndIf}

    ${NSD_CreateLabel} 0 0 100% 24u "Color Interlacer is currently running."
    Pop $0

    ${NSD_CreateLabel} 0 30u 100% 48u "The installer will close the application and restart it after installation completes. Click Next to continue or Cancel to exit the installer."
    Pop $0

    nsDialogs::Show
FunctionEnd

; Close the application when leaving the page
Function LeaveRunningAppPage
    ${If} $IsAppRunning == "1"
        DetailPrint "Closing Color Interlacer processes..."
        nsExec::Exec 'taskkill /F /IM "${TRAY_APP_EXE}"'
        nsExec::Exec 'taskkill /F /IM "${GUI_APP_EXE}"'
        nsExec::Exec 'taskkill /F /IM "${OVERLAY_APP_EXE}"'
        Sleep 1000 ; Wait for processes to close
    ${EndIf}
FunctionEnd

Section Uninstall
    ; Remove files
    Delete "$INSTDIR\${TRAY_APP_EXE}"
    Delete "$INSTDIR\${GUI_APP_EXE}"
    Delete "$INSTDIR\${OVERLAY_APP_EXE}"
    Delete "$INSTDIR\Uninstall.exe"

    ; Remove icon files
    Delete "$INSTDIR\assets\icons\icon.ico"
    Delete "$INSTDIR\assets\icons\icon-2048.png"
    RMDir "$INSTDIR\assets\icons"
    RMDir "$INSTDIR\assets"

    ; Remove directory
    RMDir "$INSTDIR"

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
