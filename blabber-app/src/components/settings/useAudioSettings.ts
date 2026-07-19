import { ref, watch } from 'vue';

const STORAGE_INPUT = 'audio-input-device';
const STORAGE_OUTPUT = 'audio-output-device';
const STORAGE_INPUT_LABEL = 'audio-input-device-label';
const STORAGE_OUTPUT_LABEL = 'audio-output-device-label';

export const selectedInputId = ref<string>(localStorage.getItem(STORAGE_INPUT) ?? '');
export const selectedOutputId = ref<string>(localStorage.getItem(STORAGE_OUTPUT) ?? '');
export const selectedInputLabel = ref<string>(localStorage.getItem(STORAGE_INPUT_LABEL) ?? '');
export const selectedOutputLabel = ref<string>(localStorage.getItem(STORAGE_OUTPUT_LABEL) ?? '');

watch(selectedInputId, (id) => {
    if (id) localStorage.setItem(STORAGE_INPUT, id);
});
watch(selectedOutputId, (id) => {
    if (id) localStorage.setItem(STORAGE_OUTPUT, id);
});
watch(selectedInputLabel, (label) => {
    if (label) localStorage.setItem(STORAGE_INPUT_LABEL, label);
});
watch(selectedOutputLabel, (label) => {
    if (label) localStorage.setItem(STORAGE_OUTPUT_LABEL, label);
});