@echo off
REM ============================================================
REM  NEXA - Inicializador completo
REM  Sobe: llama-server (8080) + backend (3001) + frontend (5173)
REM ============================================================
setlocal

set "PROJ=%~dp0"
set "LLAMA=C:\Users\Admin\Downloads\llama-b10453-bin-win-cpu-x64\llama-server.exe"
set "MODEL=D:\models\huggingface\adi-qwen2.5-coder-7b-kimi-q4_k_m.gguf"
set "NEXA_READY=1"

echo.
echo  ======================================================
echo    NEXA - Ambiente de Desenvolvimento
echo  ======================================================
echo.

REM ---------- PRÉ-VERIFICAÇÕES ----------
if not exist "%LLAMA%" (
    echo  ERRO: llama-server nao encontrado em "%LLAMA%"
    set "NEXA_READY="
)
if not exist "%MODEL%" (
    echo  ERRO: modelo GGUF nao encontrado em "%MODEL%"
    set "NEXA_READY="
)
where node >nul 2>nul
if errorlevel 1 (
    echo  ERRO: Node.js nao esta disponivel no PATH.
    set "NEXA_READY="
)
where npm >nul 2>nul
if errorlevel 1 (
    echo  ERRO: npm nao esta disponivel no PATH.
    set "NEXA_READY="
)
if not exist "%PROJ%backend\node_modules" (
    echo  ERRO: dependencias do backend ausentes. Execute npm install na pasta backend.
    set "NEXA_READY="
)
if not exist "%PROJ%frontend\node_modules" (
    echo  ERRO: dependencias do frontend ausentes. Execute npm install na pasta frontend.
    set "NEXA_READY="
)
if not defined NEXA_READY (
    echo.
    echo  NEXA nao foi iniciado. Corrija os itens acima e tente novamente.
    endlocal
    exit /b 1
)

REM ---------- 1. LLCAMA-SERVER (motor de IA local) ----------
echo  [1/3] Iniciando llama-server (IA local) na porta 8080...
start "NEXA llama-server" "%LLAMA%" -m "%MODEL%" -c 2048 --host 127.0.0.1 --port 8080 -ngl 10
echo        Aguardando o motor carregar o modelo...
timeout /t 5 /nobreak >nul

REM ---------- 2. BACKEND (API Express) ----------
echo  [2/3] Iniciando backend (API) na porta 3001...
start "NEXA Backend" /D "%PROJ%backend" cmd /k node src\server.js
timeout /t 2 /nobreak >nul

REM ---------- 3. FRONTEND (Vite) ----------
echo  [3/3] Iniciando frontend (Vite) na porta 5173...
start "NEXA Frontend" /D "%PROJ%frontend" cmd /k npm run dev

echo.
echo  ======================================================
echo    NEXA inicializado. Acesse:
echo      Frontend :  http://localhost:5173
echo      Backend  :  http://localhost:3001/api/chat/status
echo      IA local :  http://127.0.0.1:8080/health
echo  ======================================================
echo.
endlocal
