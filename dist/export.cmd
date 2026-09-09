@echo off
rem ============================================================
rem  tsbak - EXPORT des taches planifiees Windows
rem  Compatible : Windows Server 2008 R2 a 2025+.
rem  Conseil : clic droit > "Executer en tant qu'administrateur"
rem  pour exporter aussi les taches des autres comptes.
rem ============================================================
setlocal
set "BIN=%~dp0tsbak.exe"
if not exist "%BIN%" (
  echo ERREUR: tsbak.exe doit se trouver dans le meme dossier que ce script.
  pause
  exit /b 2
)

set /p "DEST=Dossier d'export (ex: C:\tsbak\export-2026-01-01) : "
if "%DEST%"=="" ( echo Aucun dossier fourni. & pause & exit /b 2 )
if not exist "%DEST%" mkdir "%DEST%"
if not exist "%~dp0logs" mkdir "%~dp0logs"

set "TS=%TIME::=%"
set "TS=%TS: =0%"
set "LOG=%~dp0logs\export-%DATE:/=-%-%TS%.log"

echo Export en cours... (journal : "%LOG%")
echo ===== [%DATE% %TIME%] Export vers "%DEST%" ===== >  "%LOG%"
"%BIN%" export "%DEST%" >> "%LOG%" 2>&1
set "CODE=%ERRORLEVEL%"
echo ===== [%DATE% %TIME%] Fin de l'export (code %CODE%) ===== >> "%LOG%"

type "%LOG%"
echo.
if "%CODE%"=="0" (
  echo Verification de l'integrite de l'archive...
  "%BIN%" validate "%DEST%"
  echo.
  echo OK. Archive pret a etre copiee / importee ailleurs.
) else (
  echo ECHEC de l'export (code %CODE%). Detail dans "%LOG%".
)
pause
endlocal