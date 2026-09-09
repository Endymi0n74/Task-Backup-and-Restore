@echo off
rem ============================================================
rem  tsbak - VALIDATION d'un dossier d'export
rem  Verifie manifest.json + empreintes SHA-256 + XML bien formes.
rem ============================================================
setlocal
set "BIN=%~dp0tsbak.exe"
if not exist "%BIN%" (
  echo ERREUR: tsbak.exe doit se trouver dans le meme dossier que ce script.
  pause
  exit /b 2
)

set /p "DIR=Dossier d'export a verifier : "
if "%DIR%"=="" ( echo Aucun dossier fourni. & pause & exit /b 2 )

"%BIN%" validate "%DIR%"
echo.
pause
endlocal