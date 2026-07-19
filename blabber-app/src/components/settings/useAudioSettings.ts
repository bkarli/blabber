import { ref, watch } from 'vue';

const STORAGE_INPUT = 'audio-input-device';
const STORAGE_OUTPUT = 'audio-output-device';

export const selectedInputId = ref<string>(localStorage.getItem(STORAGE_INPUT) ?? '');
export const selectedOutputId = ref<string>(localStorage.getItem(STORAGE_OUTPUT) ?? '');

watch(selectedInputId, (id) => {
    if (id) localStorage.setItem(STORAGE_INPUT, id);
});
watch(selectedOutputId, (id) => {
    if (id) localStorage.setItem(STORAGE_OUTPUT, id);
});
