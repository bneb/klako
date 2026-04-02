import os

def process_file(filepath):
    try:
        with open(filepath, 'r') as f:
            content = f.read()
    except UnicodeDecodeError:
        return False

    orig_content = content
    
    # Provide specific capitalization replacements
    content = content.replace("KlakoApiClient", "KlaApiClient")
    content = content.replace("KlakoApi", "KlaApi")
    content = content.replace("klako_provider", "kla_provider")
    content = content.replace("Klako Tests", "Kla Tests")
    content = content.replace("ProjectKlako", "ProjectKla")
    content = content.replace("UserKlako", "UserKla")
    content = content.replace("klako", "kla")
    content = content.replace("Klako", "Kla")
    
    if content != orig_content:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False

def main():
    skip_dirs = {'.git', 'target', 'node_modules', '.gemini'}
    changed_files = []
    
    for root, dirs, files in os.walk('.'):
        dirs[:] = [d for d in dirs if d not in skip_dirs]
        for file in files:
            filepath = os.path.join(root, file)
            if filepath.endswith('.py') and file == 'rename_kla2.py':
                continue
            if process_file(filepath):
                changed_files.append(filepath)

    for f in changed_files:
        print(f"Updated {f}")

if __name__ == '__main__':
    main()
