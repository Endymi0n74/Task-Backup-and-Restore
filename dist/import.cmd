@echo off
rem ============================================================
rem  tsbak - IMPORT (restauration) des taches planifiees Windows
rem  Compatible : Windows Server 2008 R2 a 2025+.
rem  IMPORTANT : ce script doit tourner EN ADMINISTRATEUR
rem  (clic droit > "Executer en tant qu'administrateur").
rem ============================================================
setlocal
set "BIN=%~dp0tsbak.exe"
if not exist "%BIN%" (
  echo ERREUR: tsbak.exe doit se trouver dans le meme dossier que ce script.
  pause
  exit /b 2
)

set /p "DIR=Dossier d'export a reimporter (celui qui contient manifest.json) : "
if "%DIR%"=="" ( echo Aucun dossier fourni. & pause & exit /b 2 )
if not exist "%DIR%\manifest.json" (
  echo ERREUR: manifest.json introuvable dans "%DIR%".
  pause
  exit /b 2
)

set /p "FOLDER=Dossier cible du planificateur (vide = memes dossiers que l'export, ex: \Restauration-2026) : "
set /p "PW=Fichier de mots de passe (vide = aucun; format : DOMAINE\user=motdepasse, un par ligne) : "
set /p "DRY=Simulation seule, sans rien ecrire ? (O/n) : "

set "ARGS=import "%DIR%""
if not "%FOLDER%"=="" set "ARGS=%ARGS% --folder "%FOLDER%""
if not "%PW%"=="" set "ARGS=%ARGS% --password-file "%PW%""
if /i not "%DRY%"=="n" set "ARGS=%ARGS% --dry-run"

if not exist "%~dp0logs" mkdir "%~dp0logs"
set "TS=%TIME::=%"
set "TS=%TS: =0%"
set "LOG=%~dp0logs\import-%DATE:/=-%-%TS%.log"

echo.
echo Commande : "%BIN%" %ARGS%
echo Import en cours... (journal : "%LOG%")
echo ===== [%DATE% %TIME%] Import depuis "%DIR%" ===== >  "%LOG%"
"%BIN%" %ARGS% >> "%LOG%" 2>&1
set "CODE=%ERRORLEVEL%"
echo ===== [%DATE% %TIME%] Fin de l'import (code %CODE%) ===== >> "%LOG%"

type "%LOG%"
echo.
if "%CODE%"=="0" (
  echo IMPORT TERMINE SANS ERREUR.
) else (
  echo ATTENTION : l'import s'est termine avec le code %CODE% ^(voir rapport ci-dessus^).
)
echo Journal : "%LOG%"
pause
endlocal