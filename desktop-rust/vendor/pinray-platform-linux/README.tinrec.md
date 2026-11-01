# Ajuste local do Tinrec — Pinray Linux 0.2.7

Origem: https://github.com/Itz-Agasta/pinray, crate pinray-platform-linux 0.2.7.
Licença MIT original incluída em LICENSE.

A única alteração funcional restringe os tipos do XDG ScreenCast conforme o
VideoCaptureTarget: Display solicita `types=1`, Window solicita `types=2`.
A versão original sempre solicitava `types=3`, mesmo no modo de janela.
O recorte de área é aplicado pelo motor Tinrec antes da codificação; não depende
do crop_rect do backend Wayland. Os demais módulos permanecem os da dependência.
