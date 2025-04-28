@ECHO OFF

ECHO.This script will convert the icon images to the ".ico" format.
ECHO.
ECHO.Change current directory to this script's directory: "%~dp0"
pushd "%~dp0"

ECHO.Creating "ico" folder
mkdir ico >nul 2>&1
ECHO.

for %%f in (*.jpeg) do (
  ECHO.Converting %%f to ".ico" file at: "ico/%%~nf.ico"
  magick "%%f" -define icon:auto-resize=256,64,48,32,16 "ico/%%~nf.ico"
)

for %%f in (*.jpg) do (
  ECHO.Converting %%f to ".ico" file at: "ico/%%~nf.ico"
  magick "%%f" -define icon:auto-resize=256,64,48,32,16 "ico/%%~nf.ico"
)

for %%f in (*.png) do (
  ECHO.Converting %%f to ".ico" file at: "ico/%%~nf.ico"
  magick "%%f" -define icon:auto-resize=256,64,48,32,16 "ico/%%~nf.ico"
)

ECHO.
ECHO.Converted all images.
ECHO.


popd

ECHO.Pausing for 5 seconds so we can see errors:
timeout /t 5
ECHO.