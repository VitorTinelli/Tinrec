# Ajuste local do Tinrec — Pinray Linux 0.2.7

Origem: https://github.com/Itz-Agasta/pinray, crate pinray-platform-linux 0.2.7.
Licença MIT original incluída em LICENSE.

Os tipos do XDG ScreenCast seguem o VideoCaptureTarget: um Display explícito
solicita `types=1`, Window solicita `types=2`, e Display auto solicita `types=3`
para deixar o usuário escolher a fonte no seletor do compositor. A versão
original sempre solicitava `types=3`.

No desktop Linux, Tela e Área usam Display auto. Em Wayland, Área utiliza o
stream que o portal entregou sem executar um segundo seletor no aplicativo.
Janela usa a solicitação de janela. O modo área da CLI continua disponível com --crop. Os demais módulos permanecem os da dependência.
