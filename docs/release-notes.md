## Quark 1.3.2 — Navigation, lisibilité et suivi des sessions

### Navigation et bibliothèque

- L'accueil dispose de son propre onglet dans la barre supérieure.
- La bibliothèque propose un contrôle segmenté **Grille / Liste**, un bouton **Ajouter** explicite et une recherche qui s'adapte aux fenêtres plus étroites.
- La fiche d'un jeu rassemble les actions moins fréquentes dans un menu **Plus d'actions** ; les informations restent accessibles directement et le retrait demande toujours confirmation.

### Temps de jeu plus résilient

- Les sessions en cours sont enregistrées périodiquement afin de conserver le temps déjà joué après un arrêt inattendu.
- Les points de contrôle et l'enregistrement final d'une même session sont idempotents : le temps n'est pas compté deux fois. Un test dédié couvre cette propriété.

### Lisibilité des thèmes

- Les couleurs des textes secondaires sont renforcées dans les thèmes.
- Des tests vérifient le contraste des textes secondaires et tertiaires sur les fonds principaux de chaque thème selon le seuil WCAG AA 4,5:1.
