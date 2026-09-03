#ifndef apple_intelligence_bridge_h
#define apple_intelligence_bridge_h

// C-compatible function declarations for Swift bridge

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    char* response;
    int success; // 0 for failure, 1 for success
    char* error_message; // Only valid when success = 0
} AppleLLMResponse;

// Check if Apple Intelligence is available on the device
int is_apple_intelligence_available(void);

// Process text using Apple's on-device LLM with separate system prompt and user content
AppleLLMResponse* process_text_with_system_prompt_apple(const char* system_prompt, const char* user_content, int max_tokens);

// Classify "heard -> meant" correction pairs by kind; response is a JSON array of {meant, kind}
AppleLLMResponse* check_vocabulary_apple(const char* instructions, const char* user_content);

// Free memory allocated by the Apple LLM response
void free_apple_llm_response(AppleLLMResponse* response);

#ifdef __cplusplus
}
#endif

#endif /* apple_intelligence_bridge_h */