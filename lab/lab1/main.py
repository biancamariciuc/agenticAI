from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:11434/v1",
    api_key="ollama"
)

def chat_loop():
    messages = [
        {
            "role": "system",
            "content": "You are a precise technical tutor. Respond in 2 sentences maximum."
        }
    ]

    print("--- Chat session started. Type 'exit' or 'quit' to end. ---\n")

    while True:
        try:
            user_input = input("You: ").strip()

            if user_input.lower() in ("exit", "quit"):
                print("Ending session. Goodbye!")
                break

            messages.append({"role": "user", "content": user_input})

            response = client.chat.completions.create(
                model="qwen2.5:1.5b",
                temperature=0.2,
                messages=messages
            )

            assistant_reply = response.choices[0].message.content

            print(f"\nAssistant: {assistant_reply}\n")

            # 7. Crucial: Append assistant reply back into history to maintain state
            messages.append({"role": "assistant", "content": assistant_reply})

        except KeyboardInterrupt:
            # Gracefully handle Ctrl+C
            print("\nSession interrupted. Goodbye!")
            break
        except Exception as e:
            print(f"\nError interacting with Ollama: {e}\n")
            break

if __name__ == "__main__":
    chat_loop()