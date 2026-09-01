/**
 * Model Vision Capability Detection.
 *
 * Distinguishes multimodal vision-capable models (e.g., Codex, GPT-4o, Claude 3.5 Sonnet,
 * Gemini 1.5/2.0, Qwen-VL, LLaVA, Pixtral, Llama 3.2 Vision) from text-only models.
 */

export function isVisionModel(
  modelName: string | null | undefined,
  providerId?: string | null,
): boolean {
  const normProv = providerId?.toLowerCase();

  // 1. Native multimodal flagship providers default to vision-capable
  if (normProv === "claude" || normProv === "codex" || normProv === "grok" || normProv === "gemini") {
    // Only exclude explicit legacy text-only models
    if (modelName) {
      const lower = modelName.toLowerCase();
      if (
        lower.startsWith("text-davinci") ||
        lower.startsWith("code-davinci") ||
        lower.startsWith("text-curie") ||
        lower.startsWith("text-babbage") ||
        lower.startsWith("text-ada")
      ) {
        return false;
      }
    }
    return true;
  }

  // If no modelName provided and provider is not flagship, default to false
  if (!modelName) {
    return false;
  }

  const lower = modelName.toLowerCase();

  // 2. Explicit multimodal/vision tags across all catalogs (OpenCode, Ollama, OpenRouter, etc.)
  if (
    lower.includes("vision") ||
    lower.includes("image") ||
    lower.includes("multimodal") ||
    lower.includes("-vl") ||
    lower.includes("/vl") ||
    lower.includes("_vl") ||
    lower.includes("vl-") ||
    lower.includes("vlm") ||
    lower.includes("pixtral") ||
    lower.includes("llava") ||
    lower.includes("florence") ||
    lower.includes("internvl") ||
    lower.includes("minicpm-v") ||
    lower.includes("minicpm") ||
    lower.includes("cogvlm") ||
    lower.includes("bakllava") ||
    lower.includes("molmo") ||
    lower.includes("paligemma") ||
    lower.includes("pali-gemma") ||
    lower.includes("glm-4v") ||
    lower.includes("qvq") ||
    lower.includes("deepseek-vl")
  ) {
    return true;
  }

  // 3. Known vision-capable flagship families (OpenAI, Anthropic, Google, xAI, Meta, Alibaba)
  if (
    lower.includes("4o") ||
    lower.includes("gpt-4") ||
    lower.includes("gpt-5") ||
    lower.includes("o1") ||
    lower.includes("o3") ||
    lower.includes("codex") ||
    lower.includes("sonnet") ||
    lower.includes("opus") ||
    lower.includes("haiku") ||
    lower.includes("claude-3") ||
    lower.includes("claude-4") ||
    lower.includes("claude") ||
    lower.includes("gemini") ||
    lower.includes("grok") ||
    lower.includes("qwen2.5-vl") ||
    lower.includes("qwen-vl") ||
    lower.includes("qwen2-vl") ||
    lower.includes("llama-3.2-11b") ||
    lower.includes("llama-3.2-90b") ||
    lower.includes("llama-3.2") ||
    lower.includes("mistral-large")
  ) {
    return true;
  }

  return false;
}
