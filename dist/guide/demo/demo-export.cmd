@echo off
cd /d "%~dp0"
echo ============================================================
echo  EXPORT : 1 tache vers .\export-demo  (XML + manifest.json)
echo ============================================================
"..\..\tsbak.exe" export ".\export-demo" --include "\MSIAfterburner"
echo.
echo Fichiers crees dans .\export-demo :
dir /b ".\export-demo"
echo.
echo Termine - appuyez sur une touche pour fermer
pause >nul