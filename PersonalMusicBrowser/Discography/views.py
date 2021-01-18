from django.shortcuts import render
from django.views.generic import ListView

from PersonalMusicBrowser.Discography.models import Song

# Create your views here.
class SongList(ListView):
    model = Song