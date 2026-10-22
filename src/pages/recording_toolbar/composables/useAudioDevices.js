//! 录屏工具栏 - 音频设备域：系统音频 / 麦克风 / 进程音频的设备选择、开关与录制中热切换
//!
//! 从 `pages/recording_toolbar/App.vue` 机械抽出；跨域依赖（录制状态、提示）通过参数注入。

import {ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {AISettingsService, RecordingService} from '@/services/ipc.js'

export function useAudioDevices({
    canEditAudioConfig,
    rawRecordingState,
    showInlineNotice,
    showBackendErrorInSettings,
}) {
    const {t} = useI18n()

    const captureSystemAudio = ref(false);
    const captureMicrophone = ref(false);
    const isMicMuted = ref(false); // 麦克风临时静音状态
    const systemOutputId = ref(null);
    const microphoneDeviceId = ref(null);
    const systemOutputs = ref([]);
    const microphones = ref([]);
    const audioProcesses = ref([]);
    const systemAudioProcessIds = ref([]);

    let isUpdatingAudio = false;

    const onSystemAudioDeviceChange = async (deviceId) => {
      if (!canEditAudioConfig.value || isUpdatingAudio) return;
      isUpdatingAudio = true;
      try {
        const prevCapture = captureSystemAudio.value;
        const prevId = systemOutputId.value;
        const id = String(deviceId || "");
        const nextCapture = id.length > 0;
        const nextId = nextCapture ? id : null;
        captureSystemAudio.value = id.length > 0;
        systemOutputId.value = id.length > 0 ? id : null;
        if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
          try {
            if (prevCapture && nextCapture && prevId && nextId && prevId !== nextId) {
              await RecordingService.updateAudioCapture({
                captureSystemAudio: false,
                systemAudioDeviceId: prevId || "",
              });
              await RecordingService.updateAudioCapture({
                captureSystemAudio: true,
                systemAudioDeviceId: nextId || "",
              });
            } else {
              await RecordingService.updateAudioCapture({
                captureSystemAudio: captureSystemAudio.value,
                systemAudioDeviceId: systemOutputId.value || "",
              });
            }
          } catch (e) {
            captureSystemAudio.value = prevCapture;
            systemOutputId.value = prevId;
            // 切换失败可能已停止旧采集线程，尝试恢复原设备，避免后端静默停音
            try {
              await RecordingService.updateAudioCapture({
                captureSystemAudio: prevCapture,
                systemAudioDeviceId: prevId || "",
              });
            } catch (restoreErr) {
              console.error("恢复系统音频采集失败:", restoreErr);
            }
            showBackendErrorInSettings(String(e));
            return;
          }
        }
        try {
          await AISettingsService.saveSettings({
            recordingCaptureSystemAudio: captureSystemAudio.value,
            recordingSystemAudioDeviceId: systemOutputId.value || "",
          });
        } catch (e) {
          showInlineNotice(t('recordingToolbar.saveAudioSettingsFailed', {error: String(e)}), "error");
        }
      } finally {
        isUpdatingAudio = false;
      }
    };

    const onMicrophoneDeviceChange = async (deviceId) => {
      if (!canEditAudioConfig.value || isUpdatingAudio) return;
      isUpdatingAudio = true;
      try {
        const prevCapture = captureMicrophone.value;
        const prevId = microphoneDeviceId.value;
        const prevMuted = isMicMuted.value;
        const id = String(deviceId || "");
        const nextCapture = id.length > 0;
        const nextId = nextCapture ? id : null;
        captureMicrophone.value = id.length > 0;
        microphoneDeviceId.value = id.length > 0 ? id : null;

        isMicMuted.value = false;
        if (rawRecordingState.value === "recording" || rawRecordingState.value === "paused") {
          try {
            if (prevCapture && nextCapture && prevId && nextId && prevId !== nextId) {
              await RecordingService.updateAudioCapture({
                captureMicrophone: false,
                microphoneDeviceId: prevId || "",
              });
              await RecordingService.updateAudioCapture({
                captureMicrophone: true,
                microphoneDeviceId: nextId || "",
              });
            } else {
              await RecordingService.updateAudioCapture({
                captureMicrophone: captureMicrophone.value,
                microphoneDeviceId: microphoneDeviceId.value || "",
              });
            }
          } catch (e) {
            captureMicrophone.value = prevCapture;
            microphoneDeviceId.value = prevId;
            isMicMuted.value = prevMuted;
            // 切换失败可能已停止旧采集线程，尝试恢复原设备，避免后端静默停音
            try {
              await RecordingService.updateAudioCapture({
                captureMicrophone: prevCapture,
                microphoneDeviceId: prevId || "",
              });
            } catch (restoreErr) {
              console.error("恢复麦克风采集失败:", restoreErr);
            }
            showBackendErrorInSettings(String(e));
            return;
          }
        }
        try {
          await AISettingsService.saveSettings({
            recordingCaptureMicrophone: captureMicrophone.value,
            recordingMicrophoneDeviceId: microphoneDeviceId.value || "",
          });
        } catch (e) {
          showInlineNotice(t('recordingToolbar.saveMicSettingsFailed', {error: String(e)}), "error");
        }
      } finally {
        isUpdatingAudio = false;
      }
    };

    const refreshSystemOutputDevices = async () => {
      const outs = await RecordingService.listSystemOutputs();
      systemOutputs.value = Array.isArray(outs) ? outs : [];
      const def = systemOutputs.value.find((it) => it.isDefault);
      if (captureSystemAudio.value) {
        const exists = systemOutputs.value.some((it) => it.id === systemOutputId.value);
        if (!exists) {
          systemOutputId.value = def ? def.id : (systemOutputs.value[0]?.id ?? null);
        }
      } else {
        systemOutputId.value = null;
      }
    };

    const refreshMicrophoneDevices = async () => {
      const mics = await RecordingService.listAudioDevices();
      microphones.value = Array.isArray(mics) ? mics : [];
      const def = microphones.value.find((it) => it.isDefault);
      if (captureMicrophone.value) {
        const exists = microphones.value.some((it) => it.id === microphoneDeviceId.value);
        if (!exists) {
          microphoneDeviceId.value = def ? def.id : (microphones.value[0]?.id ?? null);
        }
      } else {
        microphoneDeviceId.value = null;
      }
    };

    const refreshAudioProcesses = async () => {
      const procs = await RecordingService.listAudioProcesses();
      audioProcesses.value = Array.isArray(procs) ? procs : [];
      const pidSet = new Set(audioProcesses.value.map((p) => Number(p.pid || 0)));
      systemAudioProcessIds.value = systemAudioProcessIds.value
          .map((v) => Number(v))
          .filter((v) => pidSet.has(v));
    };
    const onSystemAudioDropdownVisibleChange = async (visible) => {
      if (!visible) return;
      try {
        await refreshSystemOutputDevices();
      } catch (_e) {
      }
    };

    const onMicrophoneDropdownVisibleChange = async (visible) => {
      if (!visible) return;
      try {
        await refreshMicrophoneDevices();
      } catch (_e) {
      }
    };

    const onAudioProcessDropdownVisibleChange = async (visible) => {
      if (!visible) return;
      try {
        await refreshAudioProcesses();
      } catch (_e) {
      }
    };

    return {
        captureSystemAudio,
        captureMicrophone,
        isMicMuted,
        systemOutputId,
        microphoneDeviceId,
        systemOutputs,
        microphones,
        audioProcesses,
        systemAudioProcessIds,
        onSystemAudioDeviceChange,
        onMicrophoneDeviceChange,
        refreshSystemOutputDevices,
        refreshMicrophoneDevices,
        refreshAudioProcesses,
        onSystemAudioDropdownVisibleChange,
        onMicrophoneDropdownVisibleChange,
        onAudioProcessDropdownVisibleChange,
    }
}
