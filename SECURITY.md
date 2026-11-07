# Segurança

Para relatar uma vulnerabilidade, use **Security → Report a vulnerability** neste repositório (relato privado do GitHub). Não abra uma issue pública com detalhes de exploração.

- O Keep guarda tokens de acesso das contas de IA no Chaveiro do sistema (Mac) ou em arquivos do perfil do usuário (Windows/Linux). Nenhuma credencial, chave ou conta faz parte do código nem das releases.
- Cada release traz atestado de procedência: `gh attestation verify <arquivo> -R ttwwnn/keep` confere que o arquivo saiu do fluxo de publicação deste repositório.
